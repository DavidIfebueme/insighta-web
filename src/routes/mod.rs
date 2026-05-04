use axum::Router;
use std::sync::Arc;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .merge(crate::handlers::health::router())
        .merge(crate::handlers::profiles::router())
}
