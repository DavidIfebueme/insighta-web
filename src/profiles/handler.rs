use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::profiles::model::{CreateProfileRequest, ListProfilesQuery, SearchQuery};
use crate::profiles::search;
use crate::profiles::service;
use crate::shared::error::AppError;
use crate::shared::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/profiles",
            get(list_profiles).post(create_profile),
        )
        .route("/api/profiles/search", get(search_profiles))
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
struct PaginatedResponse<T: Serialize> {
    status: String,
    page: i64,
    limit: i64,
    total: i64,
    data: Vec<T>,
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    body: Result<Json<CreateProfileRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let body = match body {
        Ok(Json(value)) => value,
        Err(_) => return Err(AppError::UnprocessableEntity("Invalid type".to_string())),
    };

    let name = match body.name {
        Some(n) => n.trim().to_string(),
        None => return Err(AppError::BadRequest("Name is required".to_string())),
    };

    if name.is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }

    let (dto, existing) =
        service::create_profile(&state.db, CreateProfileRequest { name: Some(name) }).await?;

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
    let dto = service::get_profile(&state.db, id).await?;
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
    query: Result<Query<ListProfilesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Query(query) = query.map_err(|_| {
        AppError::UnprocessableEntity("Invalid query parameters".to_string())
    })?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 50);

    let (total, data) = service::list_profiles(
        &state.db,
        query.gender,
        query.country_id,
        query.age_group,
        query.min_age,
        query.max_age,
        query.min_gender_probability,
        query.min_country_probability,
        query.sort_by,
        query.order,
        page,
        limit,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(PaginatedResponse {
            status: "success".to_string(),
            page,
            limit,
            total,
            data,
        }),
    ))
}

async fn search_profiles(
    State(state): State<Arc<AppState>>,
    query: Result<Query<SearchQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Query(query) = query.map_err(|_| {
        AppError::UnprocessableEntity("Invalid query parameters".to_string())
    })?;

    let q = query
        .q
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("Missing or empty parameter".to_string()))?;

    if q.trim().is_empty() {
        return Err(AppError::BadRequest("Missing or empty parameter".to_string()));
    }

    let parsed = search::parse_natural_language(q)
        .ok_or_else(|| AppError::BadRequest("Unable to interpret query".to_string()))?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 50);

    let (total, data) = service::list_profiles(
        &state.db,
        parsed.gender,
        parsed.country_id,
        parsed.age_group,
        parsed.min_age,
        parsed.max_age,
        None,
        None,
        None,
        None,
        page,
        limit,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(PaginatedResponse {
            status: "success".to_string(),
            page,
            limit,
            total,
            data,
        }),
    ))
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    service::delete_profile(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
