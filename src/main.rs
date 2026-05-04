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

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let database_max_connections: u32 = std::env::var("DATABASE_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "5".to_string())
        .parse()?;
    let server_addr = std::env::var("SERVER_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let github_client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_ID must be set"))?;
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| anyhow::anyhow!("GITHUB_CLIENT_SECRET must be set"))?;
    let jwt_secret = std::env::var("JWT_SECRET")
        .map_err(|_| anyhow::anyhow!("JWT_SECRET must be set"))?;
    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(database_max_connections)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&db).await?;

    profiles::seed::seed_profiles(&db).await?;

    let state = Arc::new(AppState {
        db,
        jwt_secret,
        github_client_id,
        github_client_secret,
        base_url,
    });

    let rate_limit_state = RateLimitState::new();

    let app = Router::new()
        .merge(health::handler::router())
        .merge(auth::handler::router())
        .merge(profiles::handler::router())
        .layer(axum::middleware::from_fn(auth::middleware::api_version_middleware))
        .layer(axum::middleware::from_fn(shared::middleware::rate_limit_middleware))
        .layer(axum::Extension(rate_limit_state))
        .layer(axum::middleware::from_fn(shared::middleware::request_logging_middleware))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server running on {}", server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
