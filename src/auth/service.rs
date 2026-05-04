use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::model::{
    AuthUser, Claims, GitHubTokenResponse, GitHubUserInfo, RefreshTokenRow, TokenPair, User,
};
use crate::shared::error::AppError;

const ACCESS_TOKEN_EXPIRY_SECS: i64 = 180;
const REFRESH_TOKEN_EXPIRY_SECS: i64 = 300;

use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

static PKCE_STORE: Lazy<Mutex<HashMap<String, PkceEntry>>> = Lazy::new(|| Mutex::new(HashMap::new()));

struct PkceEntry {
    code_verifier: String,
    redirect_url: Option<String>,
}

pub fn store_pkce(state: String, code_verifier: String, redirect_url: Option<String>) {
    PKCE_STORE.lock().unwrap().insert(state, PkceEntry { code_verifier, redirect_url });
}

pub fn take_pkce(state: &str) -> Option<(String, Option<String>)> {
    PKCE_STORE.lock().unwrap().remove(state).map(|e| (e.code_verifier, e.redirect_url))
}

pub fn generate_pkce() -> (String, String) {
    let verifier: String = (0..64)
        .map(|_| {
            let byte = rand::random::<u8>();
            format!("{:02x}", byte)
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(hash);

    (verifier, challenge)
}

pub fn generate_state() -> String {
    Uuid::now_v7().to_string()
}

pub fn build_github_auth_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&scope=user:email",
        client_id, redirect_uri, state, code_challenge
    )
}

pub async fn exchange_github_code(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
    code_verifier: &str,
) -> Result<GitHubTokenResponse, AppError> {
    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "code": code,
            "code_verifier": code_verifier,
        }))
        .send()
        .await
        .map_err(|e| {
            tracing::error!("GitHub token exchange failed: {}", e);
            AppError::BadGateway("GitHub authentication failed".to_string())
        })?;

    if !resp.status().is_success() {
        return Err(AppError::BadGateway(
            "GitHub authentication failed".to_string(),
        ));
    }

    resp.json().await.map_err(|e| {
        tracing::error!("GitHub token parse failed: {}", e);
        AppError::BadGateway("GitHub authentication failed".to_string())
    })
}

pub async fn fetch_github_user(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<GitHubUserInfo, AppError> {
    let resp = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "insighta-web")
        .send()
        .await
        .map_err(|e| {
            tracing::error!("GitHub user fetch failed: {}", e);
            AppError::BadGateway("GitHub authentication failed".to_string())
        })?;

    if !resp.status().is_success() {
        return Err(AppError::BadGateway(
            "GitHub authentication failed".to_string(),
        ));
    }

    resp.json().await.map_err(|e| {
        tracing::error!("GitHub user parse failed: {}", e);
        AppError::BadGateway("GitHub authentication failed".to_string())
    })
}

pub async fn upsert_user(
    db: &PgPool,
    github_id: i64,
    username: String,
    email: Option<String>,
    avatar_url: Option<String>,
) -> Result<User, AppError> {
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, github_id, username, email, avatar_url, role, is_active, last_login_at)
        VALUES ($1, $2, $3, $4, $5, 'analyst', true, NOW())
        ON CONFLICT (github_id) DO UPDATE SET
            username = EXCLUDED.username,
            email = EXCLUDED.email,
            avatar_url = EXCLUDED.avatar_url,
            last_login_at = NOW()
        RETURNING *
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(github_id.to_string())
    .bind(&username)
    .bind(&email)
    .bind(&avatar_url)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(user)
}

pub fn generate_access_token(user: &User, jwt_secret: &str) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user.id.to_string(),
        role: user.role.clone(),
        exp: now + ACCESS_TOKEN_EXPIRY_SECS,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(e.into()))
}

pub fn generate_refresh_token(_user: &User) -> String {
    Uuid::now_v7().to_string()
}

pub async fn store_refresh_token(
    db: &PgPool,
    user_id: Uuid,
    token: &str,
) -> Result<(), AppError> {
    let token_hash = hash_token(token);
    let expires_at = Utc::now() + chrono::Duration::seconds(REFRESH_TOKEN_EXPIRY_SECS);

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .execute(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(())
}

pub fn validate_access_token(token: &str, jwt_secret: &str) -> Result<AuthUser, AppError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))?;

    Ok(AuthUser {
        user_id: data.claims.sub.parse().map_err(|_| {
            AppError::Unauthorized("Invalid token subject".to_string())
        })?,
        role: data.claims.role,
        is_active: true,
    })
}

pub async fn validate_refresh_token(
    db: &PgPool,
    token: &str,
) -> Result<RefreshTokenRow, AppError> {
    let token_hash = hash_token(token);

    let row = sqlx::query_as::<_, RefreshTokenRow>(
        "SELECT * FROM refresh_tokens WHERE token_hash = $1 AND expires_at > NOW()",
    )
    .bind(&token_hash)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::Unauthorized("Invalid or expired refresh token".to_string()))?;

    Ok(row)
}

pub async fn revoke_refresh_token(db: &PgPool, token_hash: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
        .bind(token_hash)
        .execute(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(())
}

pub async fn get_user_by_id(db: &PgPool, id: Uuid) -> Result<User, AppError> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))
}

pub async fn issue_token_pair(db: &PgPool, user: &User, jwt_secret: &str) -> Result<TokenPair, AppError> {
    let access_token = generate_access_token(user, jwt_secret)?;
    let refresh_token = generate_refresh_token(user);
    store_refresh_token(db, user.id, &refresh_token).await?;

    Ok(TokenPair {
        access_token,
        refresh_token,
    })
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}
