use moka::sync::Cache;
use sqlx::PgPool;
use tokio::sync::Semaphore;

use crate::profiles::service::CachedQueryResult;

pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub base_url: String,
    pub cache: Cache<String, CachedQueryResult>,
    pub upload_semaphore: Semaphore,
}
