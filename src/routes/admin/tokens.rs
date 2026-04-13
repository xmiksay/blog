use axum::extract::{Extension, Form, Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use minijinja::context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, Set};

use crate::auth;
use crate::entity::token;
use crate::routes::build_menu;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}/smazat", post(delete))
}

#[derive(serde::Serialize)]
struct TokenView {
    id: i32,
    nonce_prefix: String,
    label: Option<String>,
    is_service: bool,
    expires_at: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct TokenForm {
    pub label: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
) -> impl IntoResponse {
    let tokens: Vec<TokenView> = token::Entity::find()
        .filter(token::Column::UserId.eq(user_id))
        .filter(token::Column::IsService.eq(true))
        .order_by_desc(token::Column::Id)
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|t| TokenView {
            id: t.id,
            nonce_prefix: format!("{}...", &t.nonce[..8.min(t.nonce.len())]),
            label: t.label.clone(),
            is_service: t.is_service,
            expires_at: t.expires_at.map(|e| e.to_string()),
        })
        .collect();
    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/tokens.html").unwrap();
    Html(
        tmpl.render(context! { tokens, menu, logged_in => true })
            .unwrap(),
    )
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Form(form): Form<TokenForm>,
) -> Html<String> {
    let nonce = auth::generate_token();

    token::ActiveModel {
        nonce: Set(nonce.clone()),
        user_id: Set(user_id),
        expires_at: Set(None),
        label: Set(form.label.filter(|s| !s.is_empty())),
        is_service: Set(true),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();

    let tokens: Vec<TokenView> = token::Entity::find()
        .filter(token::Column::UserId.eq(user_id))
        .filter(token::Column::IsService.eq(true))
        .order_by_desc(token::Column::Id)
        .all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|t| TokenView {
            id: t.id,
            nonce_prefix: format!("{}...", &t.nonce[..8.min(t.nonce.len())]),
            label: t.label.clone(),
            is_service: t.is_service,
            expires_at: t.expires_at.map(|e| e.to_string()),
        })
        .collect();

    let menu = build_menu(&state.db, true).await;
    let tmpl = state.tmpl.get_template("admin/tokens.html").unwrap();
    Html(
        tmpl.render(context! { tokens, menu, logged_in => true, new_token => nonce })
            .unwrap(),
    )
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    if let Ok(Some(tok)) = token::Entity::find_by_id(id).one(&state.db).await {
        if tok.user_id == user_id && tok.is_service {
            tok.delete(&state.db).await.ok();
        }
    }
    Redirect::to("/admin/tokeny")
}
