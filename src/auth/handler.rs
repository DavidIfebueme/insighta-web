use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use std::sync::Arc;

use crate::auth::model::AuthUser;
use crate::auth::model::{RefreshRequest, RefreshResponse};
use crate::auth::service;
use crate::shared::error::AppError;
use crate::shared::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/github", get(github_auth))
        .route("/auth/github/callback", get(github_callback))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    redirect_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
    code_verifier: Option<String>,
    redirect_url: Option<String>,
}

fn build_cookie_headers(access_token: &str, refresh_token: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            header::SET_COOKIE.as_str(),
            format!(
                "access_token={}; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=180",
                access_token
            ),
        ),
        (
            header::SET_COOKIE.as_str(),
            format!(
                "refresh_token={}; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=300",
                refresh_token
            ),
        ),
    ]
}

fn clear_cookie_headers() -> Vec<(&'static str, String)> {
    vec![
        (
            header::SET_COOKIE.as_str(),
            "access_token=; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=0".to_string(),
        ),
        (
            header::SET_COOKIE.as_str(),
            "refresh_token=; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=0".to_string(),
        ),
    ]
}

async fn github_auth(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuthQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (code_verifier, code_challenge) = service::generate_pkce();
    let state_val = service::generate_state();

    service::store_pkce(
        state_val.clone(),
        code_verifier.clone(),
        query.redirect_url.clone(),
    );

    let redirect_uri = format!("{}/auth/github/callback", state.base_url);

    let url = service::build_github_auth_url(
        &state.github_client_id,
        &redirect_uri,
        &state_val,
        &code_challenge,
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "data": {
                "url": url,
                "state": state_val,
                "code_verifier": code_verifier,
            }
        })),
    ))
}

async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (code_verifier, stored_redirect) = if let Some(cv) = query.code_verifier {
        (cv, None)
    } else {
        service::take_pkce(&query.state).unwrap_or_default()
    };

    let redirect_url = query.redirect_url.or(stored_redirect);

    if code_verifier.is_empty() {
        return Err(AppError::BadRequest("Missing code verifier".to_string()));
    }

    let client = reqwest::Client::new();

    let gh_token = service::exchange_github_code(
        &client,
        &state.github_client_id,
        &state.github_client_secret,
        &query.code,
        &code_verifier,
    )
    .await?;

    let gh_user = service::fetch_github_user(&client, &gh_token.access_token).await?;

    let user = service::upsert_user(
        &state.db,
        gh_user.id,
        gh_user.login,
        gh_user.email,
        gh_user.avatar_url,
    )
    .await?;

    if !user.is_active {
        return Err(AppError::Forbidden("Account is disabled".to_string()));
    }

    let tokens = service::issue_token_pair(&state.db, &user, &state.jwt_secret).await?;

    if let Some(ref redir) = redirect_url {
        let cookies = build_cookie_headers(&tokens.access_token, &tokens.refresh_token);
        let mut response = Redirect::to(redir).into_response();
        for (name, value) in cookies {
            response.headers_mut().append(name, value.parse().unwrap());
        }
        Ok(response)
    } else {
        Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "data": {
                    "user": {
                        "id": user.id,
                        "username": user.username,
                        "role": user.role,
                        "avatar_url": user.avatar_url,
                    },
                    "access_token": tokens.access_token,
                    "refresh_token": tokens.refresh_token,
                }
            })),
        )
            .into_response())
    }
}

async fn refresh_token(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(body): Json<RefreshRequest>,
) -> Result<impl IntoResponse, AppError> {
    let refresh = if !body.refresh_token.is_empty() {
        body.refresh_token.clone()
    } else {
        jar.get("refresh_token")
            .map(|c| c.value().to_string())
            .unwrap_or_default()
    };

    if refresh.is_empty() {
        return Err(AppError::Unauthorized("Missing refresh token".to_string()));
    }

    let token_row = service::validate_refresh_token(&state.db, &refresh).await?;

    service::revoke_refresh_token(&state.db, &token_row.token_hash).await?;

    let user = service::get_user_by_id(&state.db, token_row.user_id).await?;

    if !user.is_active {
        return Err(AppError::Forbidden("Account is disabled".to_string()));
    }

    let tokens = service::issue_token_pair(&state.db, &user, &state.jwt_secret).await?;

    let cookie_headers = build_cookie_headers(&tokens.access_token, &tokens.refresh_token);
    let mut response = (
        StatusCode::OK,
        Json(RefreshResponse {
            status: "success".to_string(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        }),
    )
        .into_response();

    for (name, value) in cookie_headers {
        response.headers_mut().append(name, value.parse().unwrap());
    }

    Ok(response)
}

async fn logout(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    jar: CookieJar,
    Json(body): Json<RefreshRequest>,
) -> Result<impl IntoResponse, AppError> {
    let refresh = if !body.refresh_token.is_empty() {
        body.refresh_token.clone()
    } else {
        jar.get("refresh_token")
            .map(|c| c.value().to_string())
            .unwrap_or_default()
    };

    if !refresh.is_empty()
        && let Ok(token_row) = service::validate_refresh_token(&state.db, &refresh).await
        && token_row.user_id == auth_user.user_id
    {
        let _ = service::revoke_refresh_token(&state.db, &token_row.token_hash).await;
    }

    let cookie_headers = clear_cookie_headers();
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "message": "Logged out"
        })),
    )
        .into_response();

    for (name, value) in cookie_headers {
        response.headers_mut().append(name, value.parse().unwrap());
    }

    Ok(response)
}

async fn me(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let user = service::get_user_by_id(&state.db, auth_user.user_id).await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "data": {
                "id": user.id,
                "username": user.username,
                "email": user.email,
                "avatar_url": user.avatar_url,
                "role": user.role,
                "is_active": user.is_active,
                "last_login_at": user.last_login_at,
                "created_at": user.created_at,
            }
        })),
    ))
}
