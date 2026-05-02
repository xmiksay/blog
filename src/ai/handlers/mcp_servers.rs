use std::collections::HashMap;

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::{Value, json};

use crate::auth::SESSION_COOKIE;
use crate::entity::user_mcp_server;
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct McpServerView {
    pub id: i32,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub forward_user_token: bool,
    pub headers: HashMap<String, String>,
    pub created_at: String,
}

impl From<&user_mcp_server::Model> for McpServerView {
    fn from(m: &user_mcp_server::Model) -> Self {
        let headers = m
            .headers
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        Self {
            id: m.id,
            name: m.name.clone(),
            url: m.url.clone(),
            enabled: m.enabled,
            forward_user_token: m.forward_user_token,
            headers,
            created_at: m.created_at.to_string(),
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CreateMcpServer {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub forward_user_token: Option<bool>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(serde::Deserialize)]
pub struct UpdateMcpServer {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub forward_user_token: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    jar: CookieJar,
) -> ApiResult<Json<Value>> {
    let token = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();

    let registered = user_mcp_server::Entity::find()
        .filter(user_mcp_server::Column::UserId.eq(user_id))
        .order_by_asc(user_mcp_server::Column::Name)
        .all(&state.db)
        .await?;
    let registered_view: Vec<McpServerView> =
        registered.iter().map(McpServerView::from).collect();

    let pool = state
        .mcp_manager
        .discover_user_servers(user_id, &token)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "user_servers": registered_view,
        "discovered": pool.servers(),
    })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Json(input): Json<CreateMcpServer>,
) -> ApiResult<(StatusCode, Json<McpServerView>)> {
    let name = input.name.trim().to_string();
    let url = input.url.trim().to_string();
    if name.is_empty() || url.is_empty() {
        return Err(ApiError::BadRequest("name and url required".into()));
    }

    let headers_json = serde_json::Value::Object(
        input
            .headers
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect(),
    );
    let now = chrono::Utc::now().fixed_offset();
    let saved = user_mcp_server::ActiveModel {
        user_id: Set(user_id),
        name: Set(name),
        url: Set(url),
        enabled: Set(input.enabled.unwrap_or(true)),
        forward_user_token: Set(input.forward_user_token.unwrap_or(false)),
        headers: Set(headers_json),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .map_err(|e| ApiError::Conflict(format!("could not save: {e}")))?;

    state.mcp_manager.invalidate_user(user_id);
    Ok((StatusCode::CREATED, Json(McpServerView::from(&saved))))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
    Json(input): Json<UpdateMcpServer>,
) -> ApiResult<Json<McpServerView>> {
    let row = user_mcp_server::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.user_id != user_id {
        return Err(ApiError::NotFound);
    }

    let mut active: user_mcp_server::ActiveModel = row.into();
    if let Some(v) = input.name {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("name cannot be empty".into()));
        }
        active.name = Set(trimmed.to_string());
    }
    if let Some(v) = input.enabled {
        active.enabled = Set(v);
    }
    if let Some(v) = input.forward_user_token {
        active.forward_user_token = Set(v);
    }
    if let Some(v) = input.url {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("url cannot be empty".into()));
        }
        active.url = Set(trimmed.to_string());
    }
    if let Some(h) = input.headers {
        let headers_json = serde_json::Value::Object(
            h.into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect(),
        );
        active.headers = Set(headers_json);
    }
    let updated = active.update(&state.db).await?;
    state.mcp_manager.invalidate_user(user_id);
    Ok(Json(McpServerView::from(&updated)))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    let row = user_mcp_server::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.user_id != user_id {
        return Err(ApiError::NotFound);
    }
    row.delete(&state.db).await?;
    state.mcp_manager.invalidate_user(user_id);
    Ok(StatusCode::NO_CONTENT)
}
