use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::{Value, json};

use crate::auth::SESSION_COOKIE;
use crate::ai::loop_driver;
use crate::ai::tool_permissions::Effect;
use crate::entity::{
    assistant_message, assistant_session, llm_model, llm_provider, tool_permission,
    user_mcp_server,
};
use crate::routes::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct SessionSummary {
    pub id: i32,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub model_id: Option<i32>,
    pub enabled_mcp_server_ids: Vec<i32>,
    pub created_at: String,
    pub updated_at: String,
}

fn parse_id_array(raw: &Value) -> Vec<i32> {
    raw.as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect())
        .unwrap_or_default()
}

fn ids_to_json(ids: &[i32]) -> Value {
    Value::Array(ids.iter().map(|n| Value::from(*n)).collect())
}

impl From<&assistant_session::Model> for SessionSummary {
    fn from(s: &assistant_session::Model) -> Self {
        Self {
            id: s.id,
            title: s.title.clone(),
            provider: s.provider.clone(),
            model: s.model.clone(),
            model_id: s.model_id,
            enabled_mcp_server_ids: parse_id_array(&s.enabled_mcp_server_ids),
            created_at: s.created_at.to_string(),
            updated_at: s.updated_at.to_string(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct MessageView {
    pub id: i32,
    pub seq: i32,
    pub role: String,
    pub content: Value,
    pub created_at: String,
}

impl From<&assistant_message::Model> for MessageView {
    fn from(m: &assistant_message::Model) -> Self {
        Self {
            id: m.id,
            seq: m.seq,
            role: m.role.clone(),
            content: m.content.clone(),
            created_at: m.created_at.to_string(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct SessionDetail {
    #[serde(flatten)]
    pub summary: SessionSummary,
    pub messages: Vec<MessageView>,
}

#[derive(serde::Deserialize)]
pub struct CreateSession {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model_id: Option<i32>,
    /// Optional explicit selection. When omitted, defaults to the user's
    /// currently-enabled MCP servers.
    #[serde(default)]
    pub enabled_mcp_server_ids: Option<Vec<i32>>,
}

#[derive(serde::Deserialize)]
pub struct UpdateSession {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model_id: Option<i32>,
    #[serde(default)]
    pub enabled_mcp_server_ids: Option<Vec<i32>>,
}

#[derive(serde::Deserialize)]
pub struct SendMessage {
    pub text: String,
}

#[derive(serde::Deserialize)]
pub struct ApprovalDecision {
    pub tool_call_id: String,
    pub approve: bool,
    /// When true, persist a `tool_permission` rule so future calls of the same
    /// tool name skip the approval prompt (allow if `approve`, deny otherwise).
    #[serde(default)]
    pub remember: bool,
}

#[derive(serde::Deserialize)]
pub struct ApproveBody {
    pub decisions: Vec<ApprovalDecision>,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
) -> ApiResult<Json<Vec<SessionSummary>>> {
    let rows = assistant_session::Entity::find()
        .filter(assistant_session::Column::UserId.eq(user_id))
        .order_by_desc(assistant_session::Column::UpdatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(SessionSummary::from).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Json(input): Json<CreateSession>,
) -> ApiResult<(StatusCode, Json<SessionSummary>)> {
    let (model_row, provider_row) = resolve_model_with_provider(&state, input.model_id).await?;

    let mcp_ids = match input.enabled_mcp_server_ids {
        Some(ids) => filter_owned_mcp_ids(&state, user_id, &ids).await?,
        None => default_user_mcp_ids(&state, user_id).await?,
    };

    let now = chrono::Utc::now().fixed_offset();
    let title = input.title.unwrap_or_else(|| "New chat".into());

    let saved = assistant_session::ActiveModel {
        user_id: Set(user_id),
        title: Set(title),
        provider: Set(provider_row.kind.clone()),
        model: Set(model_row.model.clone()),
        model_id: Set(Some(model_row.id)),
        enabled_mcp_server_ids: Set(ids_to_json(&mcp_ids)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(SessionSummary::from(&saved))))
}

async fn default_user_mcp_ids(state: &AppState, user_id: i32) -> ApiResult<Vec<i32>> {
    let rows = user_mcp_server::Entity::find()
        .filter(user_mcp_server::Column::UserId.eq(user_id))
        .filter(user_mcp_server::Column::Enabled.eq(true))
        .all(&state.db)
        .await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

/// Keep only IDs that actually belong to the user. Silently drops unknown or
/// foreign ids — prevents a session from referencing servers another user
/// owns.
async fn filter_owned_mcp_ids(
    state: &AppState,
    user_id: i32,
    ids: &[i32],
) -> ApiResult<Vec<i32>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = user_mcp_server::Entity::find()
        .filter(user_mcp_server::Column::UserId.eq(user_id))
        .filter(user_mcp_server::Column::Id.is_in(ids.to_vec()))
        .all(&state.db)
        .await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

pub async fn read(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> ApiResult<Json<SessionDetail>> {
    let session = load_owned(&state, user_id, id).await?;
    let messages = assistant_message::Entity::find()
        .filter(assistant_message::Column::SessionId.eq(id))
        .order_by_asc(assistant_message::Column::Seq)
        .all(&state.db)
        .await?;
    Ok(Json(SessionDetail {
        summary: SessionSummary::from(&session),
        messages: messages.iter().map(MessageView::from).collect(),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
    Json(input): Json<UpdateSession>,
) -> ApiResult<Json<SessionSummary>> {
    let session = load_owned(&state, user_id, id).await?;
    let mut mcp_changed = false;
    let mut active: assistant_session::ActiveModel = session.into();
    if let Some(t) = input.title {
        active.title = Set(t);
    }
    if let Some(mid) = input.model_id {
        let (model_row, provider_row) = resolve_model_with_provider(&state, Some(mid)).await?;
        active.model_id = Set(Some(mid));
        active.provider = Set(provider_row.kind);
        active.model = Set(model_row.model);
    }
    if let Some(ids) = input.enabled_mcp_server_ids {
        let owned = filter_owned_mcp_ids(&state, user_id, &ids).await?;
        active.enabled_mcp_server_ids = Set(ids_to_json(&owned));
        mcp_changed = true;
    }
    active.updated_at = Set(chrono::Utc::now().fixed_offset());
    let updated = active.update(&state.db).await?;
    if mcp_changed {
        state.mcp_manager.invalidate_session(id);
    }
    Ok(Json(SessionSummary::from(&updated)))
}

pub async fn delete_one(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    let session = load_owned(&state, user_id, id).await?;
    session.delete(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn send_message(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    jar: CookieJar,
    Path(id): Path<i32>,
    Json(input): Json<SendMessage>,
) -> ApiResult<Json<SessionDetail>> {
    let _ = load_owned(&state, user_id, id).await?;

    if input.text.trim().is_empty() {
        return Err(ApiError::BadRequest("text is required".into()));
    }

    let user_token = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();

    let next_seq = assistant_message::Entity::find()
        .filter(assistant_message::Column::SessionId.eq(id))
        .order_by_desc(assistant_message::Column::Seq)
        .one(&state.db)
        .await?
        .map(|m| m.seq + 1)
        .unwrap_or(1);

    assistant_message::ActiveModel {
        session_id: Set(id),
        seq: Set(next_seq),
        role: Set("user".into()),
        content: Set(json!({ "text": input.text })),
        created_at: Set(chrono::Utc::now().fixed_offset()),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    run_loop_and_record(&state, id, user_id, user_token).await
}

/// POST /sessions/{id}/messages/{message_id}/approve
/// Body: { decisions: [{ tool_call_id, approve }] }
/// Persists decisions on the assistant message and resumes the loop.
pub async fn approve(
    State(state): State<AppState>,
    Extension(user_id): Extension<i32>,
    jar: CookieJar,
    Path((id, message_id)): Path<(i32, i32)>,
    Json(body): Json<ApproveBody>,
) -> ApiResult<Json<SessionDetail>> {
    let _ = load_owned(&state, user_id, id).await?;

    let asst = assistant_message::Entity::find_by_id(message_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if asst.session_id != id || asst.role != "assistant" {
        return Err(ApiError::NotFound);
    }

    // Map tool_call_id → tool name from the message's `tool_calls`, used both
    // for persisting permission rules and for logging.
    let tool_name_by_id: std::collections::HashMap<String, String> = asst
        .content
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id").and_then(|v| v.as_str())?.to_string();
                    let name = tc.get("name").and_then(|v| v.as_str())?.to_string();
                    Some((id, name))
                })
                .collect()
        })
        .unwrap_or_default();

    // Persist "always allow / always deny" rules for any decision marked
    // `remember`. We only insert when no existing rule already covers the
    // exact tool name, so repeated approvals don't pile up duplicates.
    for d in &body.decisions {
        if !d.remember {
            continue;
        }
        let Some(tool_name) = tool_name_by_id.get(&d.tool_call_id) else {
            continue;
        };
        let existing = tool_permission::Entity::find()
            .filter(tool_permission::Column::UserId.eq(user_id))
            .filter(tool_permission::Column::Name.eq(tool_name.as_str()))
            .one(&state.db)
            .await?;
        let want = if d.approve { Effect::Allow } else { Effect::Deny };
        match existing {
            Some(row) if row.effect == want.as_str() => {}
            Some(row) => {
                let mut active: tool_permission::ActiveModel = row.into();
                active.effect = Set(want.as_str().to_string());
                active.update(&state.db).await?;
            }
            None => {
                tool_permission::ActiveModel {
                    user_id: Set(user_id),
                    name: Set(tool_name.clone()),
                    effect: Set(want.as_str().to_string()),
                    priority: Set(100),
                    ..Default::default()
                }
                .insert(&state.db)
                .await?;
            }
        }
    }

    // Merge new decisions into the message content.
    let mut content = asst.content.clone();
    let map = content.as_object_mut().ok_or_else(|| {
        ApiError::Internal("assistant message content is not an object".into())
    })?;
    let mut decisions: Vec<serde_json::Value> = map
        .get("decisions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for d in body.decisions {
        // Replace any existing decision for this tool_call_id, then push.
        decisions.retain(|v| {
            v.get("tool_call_id").and_then(|x| x.as_str()) != Some(d.tool_call_id.as_str())
        });
        decisions.push(json!({ "tool_call_id": d.tool_call_id, "approve": d.approve }));
    }
    map.insert("decisions".into(), Value::Array(decisions));
    map.remove("requires_approval");

    let mut active: assistant_message::ActiveModel = asst.into();
    active.content = Set(content);
    active.update(&state.db).await?;

    let user_token = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();

    run_loop_and_record(&state, id, user_id, user_token).await
}

async fn run_loop_and_record(
    state: &AppState,
    id: i32,
    user_id: i32,
    user_token: String,
) -> ApiResult<Json<SessionDetail>> {
    if let Err(e) = loop_driver::run_turn(
        &state.db,
        state.provider_registry.clone(),
        state.tool_registry.clone(),
        state.mcp_manager.clone(),
        state.ai_config.clone(),
        id,
        user_id,
        user_token,
    )
    .await
    {
        tracing::error!(error = %e, session_id = id, "loop_driver failed");
        let err_seq = assistant_message::Entity::find()
            .filter(assistant_message::Column::SessionId.eq(id))
            .order_by_desc(assistant_message::Column::Seq)
            .one(&state.db)
            .await?
            .map(|m| m.seq + 1)
            .unwrap_or(1);
        assistant_message::ActiveModel {
            session_id: Set(id),
            seq: Set(err_seq),
            role: Set("error".into()),
            content: Set(json!({ "text": e.to_string() })),
            created_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .ok();
    }

    let session = assistant_session::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let messages = assistant_message::Entity::find()
        .filter(assistant_message::Column::SessionId.eq(id))
        .order_by_asc(assistant_message::Column::Seq)
        .all(&state.db)
        .await?;
    Ok(Json(SessionDetail {
        summary: SessionSummary::from(&session),
        messages: messages.iter().map(MessageView::from).collect(),
    }))
}

async fn load_owned(
    state: &AppState,
    user_id: i32,
    id: i32,
) -> ApiResult<assistant_session::Model> {
    let session = assistant_session::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if session.user_id != user_id {
        return Err(ApiError::NotFound);
    }
    Ok(session)
}

async fn resolve_model_with_provider(
    state: &AppState,
    model_id: Option<i32>,
) -> ApiResult<(llm_model::Model, llm_provider::Model)> {
    let model_row = match model_id {
        Some(mid) => llm_model::Entity::find_by_id(mid)
            .one(&state.db)
            .await?
            .ok_or_else(|| ApiError::BadRequest(format!("model {mid} not found")))?,
        None => {
            let mut rows = llm_model::Entity::find().all(&state.db).await?;
            if rows.is_empty() {
                return Err(ApiError::BadRequest(
                    "no LLM models configured — add a provider and a model first".into(),
                ));
            }
            rows.sort_by_key(|r| (!r.is_default, r.id));
            rows.into_iter().next().unwrap()
        }
    };
    let provider_row = llm_provider::Entity::find_by_id(model_row.provider_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ApiError::Internal("model points at missing provider".into()))?;
    Ok((model_row, provider_row))
}
