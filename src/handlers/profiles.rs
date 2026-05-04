use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{CreateProfileRequest, ListProfilesQuery};
use crate::services::profile;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/profiles",
        get(list_profiles).post(create_profile),
    )
    .route("/api/profiles/{id}", get(get_profile).delete(delete_profile))
}

#[derive(Serialize)]
struct SuccessResponse<T: Serialize> {
    status: String,
    data: T,
}

#[derive(Serialize)]
struct ExistingProfileResponse<T: Serialize> {
    status: String,
    message: String,
    data: T,
}

#[derive(Serialize)]
struct ListResponse<T: Serialize> {
    status: String,
    count: usize,
    data: Vec<T>,
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    body: Result<Json<CreateProfileRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let body = match body {
        Ok(Json(value)) => value,
        Err(rejection) => {
            let msg = match rejection {
                axum::extract::rejection::JsonRejection::MissingJsonContentType(_) => {
                    "Missing JSON content type"
                }
                _ => "Invalid type",
            };
            return Err(AppError::UnprocessableEntity(msg.to_string()));
        }
    };

    let name = match body.name {
        Some(n) => n.trim().to_string(),
        None => return Err(AppError::BadRequest("Name is required".to_string())),
    };

    if name.is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let (dto, existing) =
        profile::create_profile(&state.db, CreateProfileRequest { name: Some(name) }).await?;

    if existing {
        Ok((
            StatusCode::OK,
            Json(ExistingProfileResponse {
                status: "success".to_string(),
                message: "Profile already exists".to_string(),
                data: dto,
            }),
        )
            .into_response())
    } else {
        Ok((
            StatusCode::CREATED,
            Json(SuccessResponse {
                status: "success".to_string(),
                data: dto,
            }),
        )
            .into_response())
    }
}

async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let dto = profile::get_profile(&state.db, id).await?;
    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            status: "success".to_string(),
            data: dto,
        }),
    ))
}

async fn list_profiles(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListProfilesQuery>,
) -> Result<impl IntoResponse, AppError> {
    let profiles = profile::list_profiles(&state.db, query).await?;
    let count = profiles.len();
    Ok((
        StatusCode::OK,
        Json(ListResponse {
            status: "success".to_string(),
            count,
            data: profiles,
        }),
    ))
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    profile::delete_profile(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
