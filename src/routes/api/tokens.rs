use axum::Json;
use axum::Router;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::get;

use crate::auth;
use crate::entity::token;
use crate::repo::tokens::{self as tokens_repo, DeleteError, ServiceTokenInput};
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", axum::routing::delete(delete_one))
}

#[derive(serde::Serialize)]
pub struct TokenSummary {
    pub id: i32,
    pub label: Option<String>,
    pub is_service: bool,
    pub expires_at: Option<String>,
}

#[derive(serde::Serialize)]
pub struct TokenCreated {
    #[serde(flatten)]
    pub summary: TokenSummary,
    pub nonce: String,
}

#[derive(serde::Deserialize)]
pub struct TokenInput {
    #[serde(default)]
    pub label: Option<String>,
}

fn to_summary(t: &token::Model) -> TokenSummary {
    TokenSummary {
        id: t.id,
        label: t.label.clone(),
        is_service: t.is_service,
        expires_at: t.expires_at.map(|e| e.to_string()),
    }
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
) -> ApiResult<Json<Vec<TokenSummary>>> {
    let tokens = tokens_repo::list_service_tokens(&state.db, user_id).await?;
    Ok(Json(tokens.iter().map(to_summary).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Json(input): Json<TokenInput>,
) -> ApiResult<(StatusCode, Json<TokenCreated>)> {
    let created = tokens_repo::create_service_token(
        &state.db,
        auth::generate_token,
        ServiceTokenInput {
            user_id,
            label: input.label,
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(TokenCreated {
            summary: to_summary(&created.model),
            nonce: created.nonce,
        }),
    ))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    match tokens_repo::delete_service_token(&state.db, user_id, id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(DeleteError::NotFound) => Err(ApiError::NotFound),
        Err(DeleteError::Db(e)) => Err(ApiError::from(e)),
    }
}
