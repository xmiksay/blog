use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use sea_orm::EntityTrait;

use crate::entity::tag;
use crate::repo::tags::{self as tags_repo, TagInput as RepoTagInput};
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(read).put(update).delete(delete_one))
}

#[derive(serde::Deserialize)]
pub struct TagInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<tag::Model>>> {
    Ok(Json(tags_repo::list_all(&state.db).await?))
}

pub async fn read(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<Json<tag::Model>> {
    tag::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<TagInput>,
) -> ApiResult<(StatusCode, Json<tag::Model>)> {
    if input.name.is_empty() {
        return Err(ApiError::BadRequest("name is required".into()));
    }
    let saved = tags_repo::create_tag(
        &state.db,
        RepoTagInput {
            name: input.name,
            description: input.description,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(saved)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(input): Json<TagInput>,
) -> ApiResult<Json<tag::Model>> {
    let updated = tags_repo::update_tag_by_id(&state.db, id, input.name, input.description)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(updated))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    use sea_orm::ModelTrait;
    let model = tag::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    model.delete(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
