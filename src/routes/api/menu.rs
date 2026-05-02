use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;

use crate::entity::menu;
use crate::repo::menu::{self as menu_repo, MenuInput as RepoMenuInput};
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(read).put(update).delete(delete_one))
}

#[derive(serde::Deserialize)]
pub struct MenuInput {
    pub title: String,
    pub path: String,
    pub markdown: String,
    pub order_index: i32,
    #[serde(default)]
    pub private: bool,
}

impl From<MenuInput> for RepoMenuInput {
    fn from(i: MenuInput) -> Self {
        Self {
            title: i.title,
            path: i.path,
            markdown: i.markdown,
            order_index: i.order_index,
            private: i.private,
        }
    }
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<menu::Model>>> {
    Ok(Json(menu_repo::list_all(&state.db).await?))
}

pub async fn read(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<Json<menu::Model>> {
    menu_repo::find_by_id(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<MenuInput>,
) -> ApiResult<(StatusCode, Json<menu::Model>)> {
    let saved = menu_repo::create(&state.db, input.into()).await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(input): Json<MenuInput>,
) -> ApiResult<Json<menu::Model>> {
    let updated = menu_repo::update(&state.db, id, input.into())
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(updated))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    if menu_repo::delete_by_id(&state.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
