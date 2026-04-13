use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{Redirect, Response},
};
use axum_extra::extract::CookieJar;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{entity::token, state::AppState};

pub const SESSION_COOKIE: &str = "blog_session";
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
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().r#gen();
    hex::encode(bytes)
}

/// Middleware: verify session cookie, redirect to login if invalid.
pub async fn require_login(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, Redirect> {
    let nonce = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| Redirect::to("/admin/login"))?;

    let tok = token::Entity::find()
        .filter(token::Column::Nonce.eq(&nonce))
        .filter(token::Column::IsService.eq(false))
        .one(&state.db)
        .await
        .map_err(|_| Redirect::to("/admin/login"))?
        .ok_or_else(|| Redirect::to("/admin/login"))?;

    // Check expiration
    if let Some(expires) = tok.expires_at {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
        if expires < now {
            return Err(Redirect::to("/admin/login"));
        }
    }

    req.extensions_mut().insert(tok.user_id);
    Ok(next.run(req).await)
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
