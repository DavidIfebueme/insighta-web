use axum::Router;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod health;
mod profiles;
mod shared;

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

    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(database_max_connections)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&db).await?;

    profiles::seed::seed_profiles(&db).await?;

    let state = Arc::new(AppState { db });

    let app = Router::new()
        .merge(health::handler::router())
        .merge(profiles::handler::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tracing::info!("Server running on {}", server_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
