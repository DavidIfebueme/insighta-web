use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    BadGateway(String),
    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    status: String,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, status_str, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "error", msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "error", msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "error", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "error", msg.clone()),
            AppError::UnprocessableEntity(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "error", msg.clone())
            }
            AppError::TooManyRequests(msg) => {
                (StatusCode::TOO_MANY_REQUESTS, "error", msg.clone())
            }
            AppError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, "502", msg.clone()),
            AppError::Internal(err) => {
                tracing::error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "error",
                    "Internal server error".to_string(),
                )
            }
        };

        (
            status,
            axum::Json(ErrorBody {
                status: status_str.to_string(),
                message,
            }),
        )
            .into_response()
    }
}
