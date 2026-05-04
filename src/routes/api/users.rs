use axum::Json;
use axum::Router;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};

use crate::auth;
use crate::entity::user;
use crate::repo::users::{self as users_repo, CreateError, DeleteError, NewUserInput, UpdateError};
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", axum::routing::delete(delete_one))
        .route("/{id}/password", put(change_password))
}

#[derive(serde::Serialize)]
pub struct UserSummary {
    pub id: i32,
    pub username: String,
    pub is_self: bool,
}

#[derive(serde::Deserialize)]
pub struct UserInput {
    pub username: String,
    pub password: String,
}

#[derive(serde::Deserialize)]
pub struct PasswordInput {
    pub password: String,
}

fn to_summary(u: &user::Model, current_user_id: i32) -> UserSummary {
    UserSummary {
        id: u.id,
        username: u.username.clone(),
        is_self: u.id == current_user_id,
    }
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
) -> ApiResult<Json<Vec<UserSummary>>> {
    let users = users_repo::list_all(&state.db).await?;
    Ok(Json(users.iter().map(|u| to_summary(u, user_id)).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Json(input): Json<UserInput>,
) -> ApiResult<(StatusCode, Json<UserSummary>)> {
    let username = input.username.trim();
    if username.is_empty() {
        return Err(ApiError::BadRequest("username is required".into()));
    }
    if input.password.is_empty() {
        return Err(ApiError::BadRequest("password is required".into()));
    }

    let password_hash = auth::hash_password(&input.password);
    match users_repo::create(
        &state.db,
        NewUserInput {
            username: username.to_string(),
            password_hash,
        },
    )
    .await
    {
        Ok(model) => Ok((StatusCode::CREATED, Json(to_summary(&model, user_id)))),
        Err(CreateError::Conflict) => Err(ApiError::Conflict(format!(
            "username '{username}' already exists"
        ))),
        Err(CreateError::Db(e)) => Err(ApiError::from(e)),
    }
}

pub async fn change_password(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(input): Json<PasswordInput>,
) -> ApiResult<StatusCode> {
    if input.password.is_empty() {
        return Err(ApiError::BadRequest("password is required".into()));
    }
    let hash = auth::hash_password(&input.password);
    match users_repo::update_password(&state.db, id, hash).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(UpdateError::NotFound) => Err(ApiError::NotFound),
        Err(UpdateError::Db(e)) => Err(ApiError::from(e)),
    }
}

pub async fn delete_one(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    match users_repo::delete(&state.db, user_id, id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DeleteError::SelfDelete) => Err(ApiError::BadRequest(
            "cannot delete your own account".into(),
        )),
        Err(DeleteError::NotFound) => Err(ApiError::NotFound),
        Err(DeleteError::Db(e)) => Err(ApiError::from(e)),
    }
}
