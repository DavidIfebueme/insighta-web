use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::model::AuthUser;
use crate::profiles::model::{CreateProfileRequest, ExportQuery, ListProfilesQuery, SearchQuery};
use crate::profiles::search;
use crate::profiles::service;
use crate::shared::error::AppError;
use crate::shared::pagination;
use crate::shared::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/profiles", get(list_profiles).post(create_profile))
        .route("/api/profiles/search", get(search_profiles))
        .route("/api/profiles/export", get(export_profiles))
        .route(
            "/api/profiles/{id}",
            get(get_profile).delete(delete_profile),
        )
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
    total_pages: i64,
    links: pagination::PaginationLinks,
    data: Vec<T>,
}

fn build_paginated_resp<T: Serialize>(
    base_path: &str,
    page: i64,
    limit: i64,
    total: i64,
    data: Vec<T>,
    query_parts: Vec<(String, String)>,
) -> PaginatedResponse<T> {
    let qs: Vec<String> = query_parts
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    let qs_str = qs.join("&");
    let tp = pagination::total_pages(total, limit);
    let links = pagination::build_links(base_path, page, limit, total, &qs_str);

    PaginatedResponse {
        status: "success".to_string(),
        page,
        limit,
        total,
        total_pages: tp,
        links,
        data,
    }
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    body: Result<Json<CreateProfileRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    if auth_user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

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
    _auth_user: AuthUser,
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
    _auth_user: AuthUser,
    query: Result<Query<ListProfilesQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".to_string()))?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(10).clamp(1, 50);

    let (total, data) = service::list_profiles(
        &state.db,
        query.gender.clone(),
        query.country_id.clone(),
        query.age_group.clone(),
        query.min_age,
        query.max_age,
        query.min_gender_probability,
        query.min_country_probability,
        query.sort_by.clone(),
        query.order.clone(),
        page,
        limit,
    )
    .await?;

    let mut qp = Vec::new();
    if let Some(ref g) = query.gender {
        qp.push(("gender".to_string(), g.clone()));
    }
    if let Some(ref c) = query.country_id {
        qp.push(("country_id".to_string(), c.clone()));
    }
    if let Some(ref a) = query.age_group {
        qp.push(("age_group".to_string(), a.clone()));
    }
    if let Some(ma) = query.min_age {
        qp.push(("min_age".to_string(), ma.to_string()));
    }
    if let Some(ma) = query.max_age {
        qp.push(("max_age".to_string(), ma.to_string()));
    }
    if let Some(mp) = query.min_gender_probability {
        qp.push(("min_gender_probability".to_string(), mp.to_string()));
    }
    if let Some(mp) = query.min_country_probability {
        qp.push(("min_country_probability".to_string(), mp.to_string()));
    }
    if let Some(ref s) = query.sort_by {
        qp.push(("sort_by".to_string(), s.clone()));
    }
    if let Some(ref o) = query.order {
        qp.push(("order".to_string(), o.clone()));
    }

    let resp = build_paginated_resp("/api/profiles", page, limit, total, data, qp);
    Ok((StatusCode::OK, Json(resp)))
}

async fn search_profiles(
    State(state): State<Arc<AppState>>,
    _auth_user: AuthUser,
    query: Result<Query<SearchQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".to_string()))?;

    let q = query
        .q
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("Missing or empty parameter".to_string()))?;

    if q.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Missing or empty parameter".to_string(),
        ));
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

    let mut qp = Vec::new();
    if let Some(ref qv) = query.q {
        qp.push(("q".to_string(), qv.clone()));
    }

    let resp = build_paginated_resp("/api/profiles/search", page, limit, total, data, qp);
    Ok((StatusCode::OK, Json(resp)))
}

async fn export_profiles(
    State(state): State<Arc<AppState>>,
    _auth_user: AuthUser,
    query: Result<Query<ExportQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::UnprocessableEntity("Invalid query parameters".to_string()))?;

    match query.format.as_deref() {
        Some("csv") | None => {}
        _ => {
            return Err(AppError::BadRequest(
                "Unsupported export format".to_string(),
            ));
        }
    }

    let csv_data = service::export_profiles_csv(
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
    )
    .await?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("profiles_{}.csv", timestamp);

    Ok((
        StatusCode::OK,
        [
            ("content-type", "text/csv".to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        csv_data,
    ))
}

async fn delete_profile(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if auth_user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    service::delete_profile(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
