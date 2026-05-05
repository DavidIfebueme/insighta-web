use axum::Router;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod health;
mod profiles;
mod shared;

use shared::middleware::RateLimitState;
use shared::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let database_min_connections: u32 = std::env::var("DATABASE_MIN_CONNECTIONS")
        .unwrap_or_else(|_| "5".to_string())
        .parse()?;
    let database_max_connections: u32 = std::env::var("DATABASE_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "20".to_string())
        .parse()?;
    let server_addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let github_client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_ID must be set"))?;
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_SECRET must be set"))?;
    let jwt_secret =
        std::env::var("JWT_SECRET").map_err(|_| anyhow::anyhow!("JWT_SECRET must be set"))?;
    let base_url =
        std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let db = sqlx::postgres::PgPoolOptions::new()
        .min_connections(database_min_connections)
        .max_connections(database_max_connections)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&db).await?;

    profiles::seed::seed_profiles(&db).await?;

    let cache = moka::sync::Cache::builder()
        .max_capacity(10_000)
        .time_to_idle(std::time::Duration::from_secs(300))
        .build();

    let upload_semaphore = tokio::sync::Semaphore::new(2);

    let state = Arc::new(AppState {
        db,
        jwt_secret,
        github_client_id,
        github_client_secret,
        base_url,
        cache,
        upload_semaphore,
    });

    let rate_limit_state = RateLimitState::new();

    let app = Router::new()
        .merge(health::handler::router())
        .merge(auth::handler::router())
        .merge(profiles::handler::router())
        .layer(axum::middleware::from_fn(
            auth::middleware::api_version_middleware,
        ))
        .layer(axum::middleware::from_fn(
            shared::middleware::rate_limit_middleware,
        ))
        .layer(axum::Extension(rate_limit_state))
        .layer(axum::middleware::from_fn(
            shared::middleware::request_logging_middleware,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:3000".parse().unwrap(),
                    "http://localhost:3001".parse().unwrap(),
                    "https://insighta-portal-ten.vercel.app".parse().unwrap(),
                ])
                .allow_credentials(true)
                .allow_methods([
                    http::Method::GET,
                    http::Method::POST,
                    http::Method::DELETE,
                    http::Method::OPTIONS,
                ])
                .allow_headers([
                    http::header::AUTHORIZATION,
                    http::header::CONTENT_TYPE,
                    http::header::HeaderName::from_static("x-api-version"),
                ]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server running on {}", server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
