use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;

use crate::shared::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health_check))
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

async fn health_check(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}
