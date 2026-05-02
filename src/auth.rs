use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;

use crate::{entity::token, state::AppState};

pub const SESSION_COOKIE: &str = "site_session";
pub const SESSION_HOURS: i64 = 24;

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("Hash failed")
        .to_string()
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = PasswordHash::new(hash).expect("Invalid hash");
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn generate_token() -> String {
    use rand::RngExt;
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}

/// Middleware: verify session cookie for /api/*. Returns 401 JSON on failure.
pub async fn require_login_api(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(nonce) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) else {
        return unauthorized();
    };

    let Ok(Some(tok)) = token::Entity::find()
        .filter(token::Column::Nonce.eq(&nonce))
        .filter(token::Column::IsService.eq(false))
        .one(&state.db)
        .await
    else {
        return unauthorized();
    };

    if let Some(expires) = tok.expires_at {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
        if expires < now {
            return unauthorized();
        }
    }

    req.extensions_mut().insert(tok.user_id);
    next.run(req).await
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
        .into_response()
}

/// Check if request has valid session (non-blocking, for template use).
pub async fn is_logged_in(state: &AppState, jar: &CookieJar) -> Option<i32> {
    let cookie = jar.get(SESSION_COOKIE)?;
    let nonce = cookie.value().to_string();
    let tok = token::Entity::find()
        .filter(token::Column::Nonce.eq(&nonce))
        .filter(token::Column::IsService.eq(false))
        .one(&state.db)
        .await
        .ok()
        .flatten()?;
    if let Some(expires) = tok.expires_at {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
        if expires < now {
            return None;
        }
    }
    Some(tok.user_id)
}
