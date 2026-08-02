//! #99: a spawned sub-agent gets its own `assistant_sessions` row, linked to
//! the spawning session and filed under the same engine log key.
//!
//! Two writers race to create that row — `ai::ws_bridge::child_rows` off the
//! live `OutEvent` stream, and `ai::handlers::sessions::subagent_links` off the
//! persisted log whenever a handler projects it. Each test here pins one of
//! them: the first runs the full fixture (both writers, `ON CONFLICT DO
//! NOTHING` deciding who wins), the second deliberately omits
//! `ws_bridge::spawn` so only hydration can possibly have written the row.
//!
//! DB-gated only, driven by a scripted `Llm` like the sibling
//! `tests/assistant_session_subagent_researcher.rs` — see that file's doc.

mod common;
#[path = "common/scripted.rs"]
mod scripted;

use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::{send, test_db_url};
use entanglement_core::{Llm, LlmRequest, LlmResponse, LlmStream, ToolCall, stream_from_response};
use scripted::{
    ScriptedFixture, scripted_cleanup, scripted_session, setup_scripted,
    setup_scripted_without_ws_bridge,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use site::entity::assistant_session;

/// The root spawns one `researcher` and then finishes; the child answers with
/// plain text. Same shape as the researcher flow test's script — this file
/// asserts on the DB rows the spawn leaves behind rather than the transcript.
#[derive(Default)]
struct SpawnScriptedLlm {
    calls: u32,
}

#[async_trait]
impl Llm for SpawnScriptedLlm {
    async fn stream(&mut self, req: LlmRequest<'_>) -> anyhow::Result<LlmStream> {
        self.calls += 1;
        let resp = if req.system.contains("`researcher` sub-agent") {
            LlmResponse {
                text: "2 + 2 = 4.".into(),
                tool_calls: vec![],
            }
        } else if self.calls == 1 {
            LlmResponse {
                text: String::new(),
                tool_calls: vec![ToolCall::new(
                    "spawn-1",
                    "agent_spawn",
                    r#"{"agent":"researcher","prompt":"what is 2+2"}"#,
                )],
            }
        } else {
            LlmResponse {
                text: "Researching, thanks!".into(),
                tool_calls: vec![],
            }
        };
        Ok(stream_from_response(resp))
    }
}

fn spawn_factory() -> entanglement_core::LlmFactory {
    std::sync::Arc::new(|| Box::new(SpawnScriptedLlm::default()))
}

/// Poll `GET /sessions/{id}` until the projection reports a sub-agent, and
/// return its engine `SessionId` (`agent_id`, recovered by the projection from
/// the spawn's own tool result). The child runs detached (ADR-0026), so this
/// polls rather than racing the background task.
async fn await_spawned_agent_id(fx: &ScriptedFixture, db_session_id: i32) -> String {
    let mut last = Value::Null;
    for _ in 0..30 {
        let (status, resp) = send(
            &fx.app,
            "GET",
            &format!("/assistant/sessions/{db_session_id}"),
            &fx.cookie,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "read session: {resp}");
        let found = resp["messages"]
            .as_array()
            .and_then(|messages| {
                messages.iter().find_map(|m| {
                    m["content"]["sub_agents"]
                        .as_array()?
                        .first()?
                        .get("agent_id")?
                        .as_str()
                        .map(String::from)
                })
            })
            .filter(|id| !id.is_empty());
        if let Some(id) = found {
            return id;
        }
        last = resp;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no sub-agent ever showed up in the projection: {last:#}")
}

/// Poll for the child's own row — both writers are asynchronous with respect
/// to the request that observed the spawn.
async fn await_child_row(fx: &ScriptedFixture, agent_id: &str) -> assistant_session::Model {
    for _ in 0..30 {
        let row = assistant_session::Entity::find()
            .filter(assistant_session::Column::EngineSessionId.eq(agent_id))
            .one(&fx.db)
            .await
            .expect("query assistant_sessions");
        if let Some(row) = row {
            return row;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("sub-agent session {agent_id} never got an assistant_sessions row")
}

/// The row's every derived field, checked against the parent it was spawned
/// from — `user_id` in particular, since copying it from the parent row (not
/// `user_id_from_session`, which is evicted when the child settles) is what
/// keeps `load_owned`'s ownership check working for the child's own view.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn spawned_sub_agent_gets_its_own_linked_session_row() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted(&db_url, "child-rows", spawn_factory()).await;
    let (db_session_id, root_session_id) = scripted_session(&fx).await;

    let (status, resp) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{db_session_id}/messages"),
        &fx.cookie,
        Some(json!({ "text": "research 2+2" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send_message: {resp}");

    let agent_id = await_spawned_agent_id(&fx, db_session_id).await;
    let child = await_child_row(&fx, &agent_id).await;

    assert_eq!(
        child.parent_session_id,
        Some(db_session_id),
        "the child row must point at the session that spawned it"
    );
    assert_eq!(
        child.user_id, fx.user_id,
        "user_id comes from the parent row"
    );
    assert_eq!(
        child.root_engine_session_id, root_session_id.0,
        "the child's events are filed under the root's engine session id, \
         which is what makes its transcript readable"
    );
    assert_eq!(child.agent_profile, "researcher");
    assert_eq!(child.engine_session_id, Some(agent_id));
    assert_eq!(
        child.enabled_mcp_server_ids,
        json!([]),
        "a sub-agent gets no MCP servers of its own"
    );

    // Only the root is a root: the child must not claim to be one.
    let (status, list) = send(&fx.app, "GET", "/assistant/sessions", &fx.cookie, None).await;
    assert_eq!(status, StatusCode::OK, "list sessions: {list}");
    let summary = list
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == json!(child.id)))
        .unwrap_or_else(|| panic!("child session missing from the list: {list:#}"));
    assert_eq!(summary["parent_session_id"], json!(db_session_id));
    assert_eq!(summary["root_engine_session_id"], json!(root_session_id.0));

    scripted_cleanup(&fx, db_session_id).await;
}

/// Hydration standalone: with `ws_bridge::spawn` never started, the live
/// writer cannot possibly run, so the row appearing at all proves
/// `handlers::sessions::subagent_links::hydrate_child_rows` rebuilt it from
/// the persisted log. This is the path that also repairs a `RecvError::Lagged`
/// batch, which the live writer can never recover from on its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hydration_creates_the_child_row_without_the_live_ws_writer() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted_without_ws_bridge(&db_url, "child-rows-hydrate", spawn_factory()).await;
    let (db_session_id, root_session_id) = scripted_session(&fx).await;

    let (status, resp) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{db_session_id}/messages"),
        &fx.cookie,
        Some(json!({ "text": "research 2+2" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "send_message: {resp}");

    let agent_id = await_spawned_agent_id(&fx, db_session_id).await;
    let child = await_child_row(&fx, &agent_id).await;

    assert_eq!(child.parent_session_id, Some(db_session_id));
    assert_eq!(child.user_id, fx.user_id);
    assert_eq!(child.root_engine_session_id, root_session_id.0);
    assert_eq!(child.agent_profile, "researcher");

    scripted_cleanup(&fx, db_session_id).await;
}
