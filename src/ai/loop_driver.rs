use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::{Value, json};

use crate::ai::config::{AiConfig, SYSTEM_PROMPT_PAGE_PATH};
use crate::ai::llm::registry::ProviderRegistry;
use crate::ai::llm::{
    ChatRequest, ProviderError, ProviderMessage, ProviderToolCall, ProviderToolResult,
};
use crate::ai::mcp_client::UserMcpManager;
use crate::ai::tool_permissions::{self, Effect};
use crate::ai::tool_registry::{DispatchCtx, ToolRegistry};
use crate::entity::{assistant_message, assistant_session};
use crate::repo::pages as pages_repo;

const MAX_ITERATIONS: u32 = 16;

/// Drive one user turn. Loops chat → tools → chat until the model produces a
/// tool-call-free reply or we hit a permission `Prompt` for an unapproved
/// call (in which case we mark the message `requires_approval` and return —
/// the user resumes via the `approve_tool_calls` handler).
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    db: &DatabaseConnection,
    provider_registry: Arc<ProviderRegistry>,
    tool_registry: Arc<ToolRegistry>,
    mcp_manager: Arc<UserMcpManager>,
    config: Arc<AiConfig>,
    session_id: i32,
    user_id: i32,
    user_token: String,
) -> anyhow::Result<()> {
    let session = assistant_session::Entity::find_by_id(session_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

    let resolved = match session.model_id {
        Some(mid) => provider_registry.resolve(mid).await?,
        None => provider_registry.resolve_default().await?,
    };

    let mcp_pool = mcp_manager
        .pool_for_session(session_id, user_id, &user_token)
        .await?;
    let tool_specs = tool_registry.aggregated_specs(&mcp_pool);

    let system_prompt = match pages_repo::find_by_path(db, SYSTEM_PROMPT_PAGE_PATH).await? {
        Some(page) if !page.markdown.trim().is_empty() => page.markdown,
        _ => config.system_prompt.clone(),
    };

    let model = resolved.model.clone();

    let mut iteration: u32 = 0;
    loop {
        iteration += 1;
        if iteration > MAX_ITERATIONS {
            tracing::warn!(session_id, "loop hit MAX_ITERATIONS");
            return Ok(());
        }

        let messages = assistant_message::Entity::find()
            .filter(assistant_message::Column::SessionId.eq(session_id))
            .order_by_asc(assistant_message::Column::Seq)
            .all(db)
            .await?;
        if messages.is_empty() {
            return Ok(());
        }

        // If the tail is an assistant message with pending tool_calls (no
        // matching tool_results yet), execute / await approval BEFORE calling
        // the model again.
        if let Some(asst) = find_pending_assistant(&messages) {
            let outcome = process_pending_tool_calls(
                db,
                &tool_registry,
                &mcp_pool,
                &user_token,
                session_id,
                user_id,
                &asst,
                &messages,
            )
            .await?;
            match outcome {
                ProcessOutcome::Paused => return Ok(()),
                ProcessOutcome::Continued => continue,
            }
        }

        let provider_messages = build_provider_messages(&messages);

        let request = ChatRequest {
            model: model.clone(),
            system: system_prompt.clone(),
            messages: provider_messages,
            tools: tool_specs.clone(),
        };

        let response = match resolved.provider.chat(request).await {
            Ok(r) => r,
            Err(ProviderError::Transport(e))
            | Err(ProviderError::Protocol(e))
            | Err(ProviderError::Config(e)) => {
                anyhow::bail!("Provider call failed: {e}");
            }
        };

        let next_seq = messages.iter().map(|m| m.seq).max().unwrap_or(0) + 1;
        let content = json!({
            "text": response.text,
            "tool_calls": response.tool_calls,
        });
        assistant_message::ActiveModel {
            session_id: Set(session_id),
            seq: Set(next_seq),
            role: Set("assistant".into()),
            content: Set(content),
            created_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        }
        .insert(db)
        .await?;

        if response.tool_calls.is_empty() {
            let mut sess: assistant_session::ActiveModel = session.clone().into();
            sess.updated_at = Set(chrono::Utc::now().fixed_offset());
            sess.update(db).await.ok();
            return Ok(());
        }
        // Loop — next iteration sees the new assistant message as pending.
    }
}

enum ProcessOutcome {
    Continued,
    Paused,
}

/// Detect an assistant message at the tail with tool_calls and not enough
/// tool_result rows after it.
fn find_pending_assistant(messages: &[assistant_message::Model]) -> Option<assistant_message::Model> {
    let last_asst = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")?
        .clone();
    let calls = last_asst
        .content
        .get("tool_calls")
        .and_then(|v| v.as_array())?;
    if calls.is_empty() {
        return None;
    }
    let later_results: usize = messages
        .iter()
        .filter(|m| m.seq > last_asst.seq && m.role == "tool_result")
        .count();
    if later_results >= calls.len() {
        return None;
    }
    Some(last_asst)
}

#[allow(clippy::too_many_arguments)]
async fn process_pending_tool_calls(
    db: &DatabaseConnection,
    tool_registry: &Arc<ToolRegistry>,
    mcp_pool: &Arc<crate::ai::mcp_client::McpClientPool>,
    user_token: &str,
    session_id: i32,
    user_id: i32,
    asst: &assistant_message::Model,
    messages: &[assistant_message::Model],
) -> anyhow::Result<ProcessOutcome> {
    let raw_calls = asst
        .content
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let calls: Vec<ProviderToolCall> = raw_calls
        .iter()
        .filter_map(|v| match serde_json::from_value::<ProviderToolCall>(v.clone()) {
            Ok(tc) => Some(tc),
            Err(e) => {
                tracing::error!(error = %e, value = %v, "failed to deserialize tool_call");
                None
            }
        })
        .collect();
    if calls.len() != raw_calls.len() {
        anyhow::bail!(
            "tool_calls deserialization mismatch ({} parsed of {}); refusing to silently skip",
            calls.len(),
            raw_calls.len()
        );
    }

    // Per-call decisions persisted on the assistant message by the approval
    // endpoint, keyed by tool_call_id.
    let decisions = decisions_map(asst);

    // Resolve every call's effect: explicit decision overrides rule lookup.
    let mut resolved: Vec<Effect> = Vec::with_capacity(calls.len());
    let mut any_prompt = false;
    for tc in &calls {
        let (effect, source) = if let Some(d) = decisions.get(&tc.id) {
            (*d, "decision")
        } else {
            (
                tool_permissions::resolve(db, user_id, &tc.name).await?,
                "rule",
            )
        };
        tracing::info!(
            session_id,
            user_id,
            tool = %tc.name,
            tool_call_id = %tc.id,
            effect = effect.as_str(),
            source,
            "tool permission resolved",
        );
        if effect == Effect::Prompt {
            any_prompt = true;
        }
        resolved.push(effect);
    }

    if any_prompt {
        mark_requires_approval(db, asst).await?;
        return Ok(ProcessOutcome::Paused);
    }

    // Execute (or synthesize a denial) for each call, in order, after the
    // last existing message.
    let mut current_seq = messages.iter().map(|m| m.seq).max().unwrap_or(0);
    let dispatch_ctx = DispatchCtx {
        db: db.clone(),
        session_id,
        user_id,
        user_token: user_token.to_string(),
        mcp_pool: mcp_pool.clone(),
    };
    for (i, tc) in calls.iter().enumerate() {
        current_seq += 1;
        let effect = resolved[i];
        let (output, is_error) = match effect {
            Effect::Allow => match tool_registry
                .dispatch(&tc.name, tc.args.clone(), &dispatch_ctx)
                .await
            {
                Ok(v) => (v, false),
                Err(e) => (json!({ "text": e.to_string() }), true),
            },
            Effect::Deny => (
                json!({ "text": "Tool execution denied by permission rule or user" }),
                true,
            ),
            Effect::Prompt => unreachable!("any_prompt would have returned earlier"),
        };

        assistant_message::ActiveModel {
            session_id: Set(session_id),
            seq: Set(current_seq),
            role: Set("tool_result".into()),
            content: Set(json!({
                "tool_call_id": tc.id,
                "output": output,
                "is_error": is_error,
            })),
            created_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }
    Ok(ProcessOutcome::Continued)
}

fn decisions_map(asst: &assistant_message::Model) -> HashMap<String, Effect> {
    let mut out = HashMap::new();
    let Some(arr) = asst.content.get("decisions").and_then(|v| v.as_array()) else {
        return out;
    };
    for d in arr {
        let id = d.get("tool_call_id").and_then(|v| v.as_str()).map(String::from);
        let approve = d.get("approve").and_then(|v| v.as_bool());
        if let (Some(i), Some(a)) = (id, approve) {
            out.insert(i, if a { Effect::Allow } else { Effect::Deny });
        }
    }
    out
}

async fn mark_requires_approval(
    db: &DatabaseConnection,
    asst: &assistant_message::Model,
) -> anyhow::Result<()> {
    if asst
        .content
        .get("requires_approval")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let mut content = asst.content.clone();
    if let Some(obj) = content.as_object_mut() {
        obj.insert("requires_approval".into(), Value::Bool(true));
    }
    let mut am: assistant_message::ActiveModel = asst.clone().into();
    am.content = Set(content);
    am.update(db).await?;
    Ok(())
}

fn build_provider_messages(messages: &[assistant_message::Model]) -> Vec<ProviderMessage> {
    let mut out = Vec::new();

    for m in messages {
        match m.role.as_str() {
            "user" => {
                let text = m
                    .content
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                out.push(ProviderMessage::User { text });
            }
            "assistant" => {
                let text = m
                    .content
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let tool_calls: Vec<ProviderToolCall> = m
                    .content
                    .get("tool_calls")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(ProviderMessage::Assistant { text, tool_calls });
            }
            "tool_result" => {
                let tool_call_id = m
                    .content
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let output = m
                    .content
                    .get("output")
                    .cloned()
                    .unwrap_or(Value::Null);
                let is_error = m
                    .content
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                out.push(ProviderMessage::ToolResults {
                    results: vec![ProviderToolResult {
                        tool_call_id,
                        output,
                        is_error,
                    }],
                });
            }
            _ => {}
        }
    }

    merge_tool_results(&mut out);
    out
}

fn merge_tool_results(messages: &mut Vec<ProviderMessage>) {
    let mut i = 0;
    while i + 1 < messages.len() {
        if matches!(&messages[i], ProviderMessage::ToolResults { .. })
            && matches!(&messages[i + 1], ProviderMessage::ToolResults { .. })
        {
            let next = messages.remove(i + 1);
            if let ProviderMessage::ToolResults { results: next_results } = next
                && let ProviderMessage::ToolResults { results } = &mut messages[i]
            {
                results.extend(next_results);
            }
        } else {
            i += 1;
        }
    }
}
