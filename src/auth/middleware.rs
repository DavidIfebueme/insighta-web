use axum::extract::{FromRequestParts, Request};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use crate::auth::model::AuthUser;
use crate::shared::error::AppError;
use crate::shared::state::AppState;

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
            .or_else(|| {
                parts
                    .headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|cookie_str| {
                        cookie_str
                            .split(';')
                            .find_map(|c| {
                                let c = c.trim();
                                c.strip_prefix("access_token=")
                            })
                            .map(|s| s.to_string())
                    })
            })
            .ok_or_else(|| AppError::Unauthorized("Missing authorization header".to_string()))?;

        let auth_user = crate::auth::service::validate_access_token(&token, &state.jwt_secret)?;

        let user = crate::auth::service::get_user_by_id(&state.db, auth_user.user_id).await?;
        if !user.is_active {
            return Err(AppError::Forbidden("Account is disabled".to_string()));
        }

        Ok(AuthUser {
            user_id: user.id,
            role: user.role,
            is_active: user.is_active,
        })
    }
}

#[allow(dead_code)]
pub async fn require_admin(
    auth_user: AuthUser,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if auth_user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }
    req.extensions_mut().insert(auth_user);
    Ok(next.run(req).await)
}

pub async fn api_version_middleware(req: Request, next: Next) -> Result<Response, AppError> {
    let path = req.uri().path().to_string();
    if path.starts_with("/api/") {
        let version = req
            .headers()
            .get("X-API-Version")
            .and_then(|v| v.to_str().ok());

        if version != Some("1") {
            return Err(AppError::BadRequest(
                "API version header required".to_string(),
            ));
        }
    }
    Ok(next.run(req).await)
}
