use axum::extract::{Form, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::auth;
use crate::entity::{token, user};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_form).post(login))
        .route("/logout", get(logout))
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login_form(State(state): State<AppState>) -> impl IntoResponse {
    let tmpl = state.tmpl.get_template("admin/login.html").unwrap();
    Html(tmpl.render(minijinja::context! {}).unwrap())
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<(CookieJar, Redirect), Html<String>> {
    let found = user::Entity::find()
        .filter(user::Column::Username.eq(&form.username))
        .one(&state.db)
        .await
        .unwrap();

    let Some(found) = found else {
        return Err(render_login_error(&state, "Invalid credentials"));
    };

    if !auth::verify_password(&form.password, &found.password_hash) {
        return Err(render_login_error(&state, "Invalid credentials"));
    }

    let nonce = auth::generate_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(auth::SESSION_HOURS);

    token::ActiveModel {
        nonce: Set(nonce.clone()),
        user_id: Set(found.id),
        expires_at: Set(Some(expires_at.into())),
        is_service: Set(false),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();

    let cookie = Cookie::build((auth::SESSION_COOKIE, nonce))
        .http_only(true)
        .path("/")
        .build();

    Ok((jar.add(cookie), Redirect::to("/admin")))
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(cookie) = jar.get(auth::SESSION_COOKIE) {
        let nonce = cookie.value().to_string();
        token::Entity::delete_many()
            .filter(token::Column::Nonce.eq(nonce))
            .exec(&state.db)
            .await
            .ok();
    }
    let removal = Cookie::build(auth::SESSION_COOKIE).path("/").build();
    (jar.remove(removal), Redirect::to("/admin/login"))
}

fn render_login_error(state: &AppState, msg: &str) -> Html<String> {
    let tmpl = state.tmpl.get_template("admin/login.html").unwrap();
    Html(tmpl.render(minijinja::context! { error => msg }).unwrap())
}
