use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use sea_orm::EntityTrait;

use crate::entity::image as image_entity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(serve))
        .route("/{id}/nahled", get(serve_thumbnail))
}

pub async fn serve(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    match image_entity::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(img)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            img.data,
        )
            .into_response(),
        _ => (axum::http::StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

pub async fn serve_thumbnail(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    match image_entity::Entity::find_by_id(id).one(&state.db).await {
        Ok(Some(img)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            img.thumbnail,
        )
            .into_response(),
        _ => (axum::http::StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}
