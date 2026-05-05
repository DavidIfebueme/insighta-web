use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::shared::error::AppError;

#[derive(Clone)]
pub struct RateLimitState {
    inner: Arc<Mutex<RateLimitInner>>,
}

struct RateLimitInner {
    auth_attempts: HashMap<String, Vec<Instant>>,
    user_requests: HashMap<String, Vec<Instant>>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateLimitInner {
                auth_attempts: HashMap::new(),
                user_requests: HashMap::new(),
            })),
        }
    }
}

pub async fn rate_limit_middleware(
    rate_state: axum::extract::Extension<RateLimitState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let path = req.uri().path().to_string();
    let is_auth = path.starts_with("/auth/");

    let key = if is_auth {
        let ip = req
            .headers()
            .get("x-forwarded-for")
            .or_else(|| req.headers().get("x-real-ip"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        format!("auth:{}", ip)
    } else {
        let user_id = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("anonymous")
            .to_string();
        format!("api:{}", user_id)
    };

    let limit = if is_auth { 10 } else { 60 };

    let mut inner = rate_state.inner.lock().await;
    let now = Instant::now();
    let minute_ago = now - std::time::Duration::from_secs(60);

    let attempts = if is_auth {
        inner.auth_attempts.entry(key.clone()).or_default()
    } else {
        inner.user_requests.entry(key.clone()).or_default()
    };

    attempts.retain(|t| *t > minute_ago);

    if attempts.len() >= limit {
        return Err(AppError::TooManyRequests("Rate limit exceeded".to_string()));
    }

    attempts.push(now);

    drop(inner);
    Ok(next.run(req).await)
}

pub async fn request_logging_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(req).await;

    let elapsed = start.elapsed();
    let status = response.status().as_u16();

    tracing::info!(
        method = %method,
        endpoint = %path,
        status = status,
        response_time_ms = elapsed.as_millis() as u64,
        "request"
    );

    response
}
