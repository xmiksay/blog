//! #99/#101: what a manual compaction does to a sub-agent spawned *before* it.
//!
//! A compaction forks a fresh successor engine session and repoints the root
//! row's `engine_session_id`/`root_engine_session_id` at it (ADR-0101/0110,
//! `src/ai/handlers/sessions/compact.rs`). A child spawned before that keeps
//! the *pre-compaction* key, which cuts both ways and is exactly the pair of
//! behaviors a row pointer (instead of a stored engine id) would get wrong:
//!
//! - its own transcript stays readable, out of the log it was actually written
//!   to, even though the session it ran under has been retired;
//! - it disappears from the root's post-compact view, because the successor's
//!   log has no record of a child spawned before the successor existed.
//!
//! Split from `tests/assistant_session_compact.rs` (which owns the overflow and
//! fork/retire cases) to keep both files under the 400-line cap. The `SetModel`
//! re-pin/loopback-mock arrangement is the same one that file's module doc
//! explains.

mod common;
#[path = "common/llm_mock.rs"]
mod llm_mock;
#[path = "common/scripted.rs"]
mod scripted;

use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::{send, test_db_url};
use entanglement_core::{Llm, LlmRequest, LlmResponse, LlmStream, ToolCall, stream_from_response};
use llm_mock::spawn_openai_sse_mock;
use scripted::{scripted_cleanup, scripted_session_with_model, setup_scripted_with_catalog_model};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use site::entity::{assistant_session, llm_provider};

/// What the mocked catalog endpoint answers the summarize oneshot with — the
/// compaction re-pins the source onto that model before summarizing.
const MOCK_SUMMARY: &str = "SUMMARY_MARKER: user asked, agent researched.";

/// The child's answer: the marker that proves its transcript survived the fork.
const CHILD_ANSWER: &str = "SUBAGENT_MARKER: 2 + 2 = 4.";

/// Spawns one `researcher` on the root's first turn; the child answers with
/// [`CHILD_ANSWER`]. The summarize branch is a tripwire — reaching it would
/// mean the handler's `SetModel` re-pin didn't take and the summary never went
/// to the loopback mock.
#[derive(Default)]
struct SpawnThenCompactLlm {
    calls: u32,
}

#[async_trait]
impl Llm for SpawnThenCompactLlm {
    async fn stream(&mut self, req: LlmRequest<'_>) -> anyhow::Result<LlmStream> {
        self.calls += 1;
        let resp = if req.system.contains("summarization assistant") {
            LlmResponse {
                text: "SCRIPTED_FALLBACK: the compact re-pin did not take effect".into(),
                tool_calls: vec![],
            }
        } else if req.system.contains("`researcher` sub-agent") {
            LlmResponse {
                text: CHILD_ANSWER.into(),
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
                text: "REPLY_MARKER".into(),
                tool_calls: vec![],
            }
        };
        Ok(stream_from_response(resp))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_compaction_sub_agent_stays_readable_after_the_fork() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let mock = spawn_openai_sse_mock(MOCK_SUMMARY, Duration::from_millis(0)).await;
    let (fx, provider_id, model_id) = setup_scripted_with_catalog_model(
        &db_url,
        "compact-subagent",
        std::sync::Arc::new(|| Box::new(SpawnThenCompactLlm::default()) as Box<dyn Llm>),
        200_000,
        mock.base_url.clone(),
    )
    .await;
    let (session_db_id, source_session_id) = scripted_session_with_model(&fx, model_id).await;

    let (status, resp) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{session_db_id}/messages"),
        &fx.cookie,
        Some(json!({ "text": "research 2+2" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "seed message: {resp}");

    // The child runs detached (ADR-0026); poll for its row. Re-reading the root
    // each time also drives the handler-side hydration writer.
    let mut child = None;
    for _ in 0..40 {
        child = assistant_session::Entity::find()
            .filter(assistant_session::Column::ParentSessionId.eq(session_db_id))
            .one(&fx.db)
            .await
            .expect("query assistant_sessions");
        if child.is_some() {
            break;
        }
        let _ = send(
            &fx.app,
            "GET",
            &format!("/assistant/sessions/{session_db_id}"),
            &fx.cookie,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let child = child.expect("the spawned sub-agent never got its own row");
    assert_eq!(child.root_engine_session_id, source_session_id.0);

    let (status, compacted) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{session_db_id}/compact"),
        &fx.cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "compact: {compacted}");

    let updated = assistant_session::Entity::find_by_id(session_db_id)
        .one(&fx.db)
        .await
        .expect("query assistant_sessions")
        .expect("session row still exists");
    assert_ne!(
        updated.root_engine_session_id, child.root_engine_session_id,
        "the root's log key must move with the fork while the child's stays put"
    );

    // `DbSink` appends asynchronously behind its own writer task.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (status, child_view) = send(
        &fx.app,
        "GET",
        &format!("/assistant/sessions/{}", child.id),
        &fx.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "read child: {child_view}");
    assert!(
        child_view["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["content"]["text"].as_str())
            .any(|t| t.contains(CHILD_ANSWER)),
        "a pre-compaction sub-agent's transcript must survive the fork: {child_view:#}"
    );

    let (status, root_view) = send(
        &fx.app,
        "GET",
        &format!("/assistant/sessions/{session_db_id}"),
        &fx.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "read root: {root_view}");
    let child_engine_id = child
        .engine_session_id
        .clone()
        .expect("child engine_session_id");
    assert!(
        !root_view["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["content"]["sub_agents"].as_array())
            .flatten()
            .any(|card| card["agent_id"] == json!(child_engine_id)),
        "the pre-compaction child must not appear in the successor's view: {root_view:#}"
    );

    scripted_cleanup(&fx, session_db_id).await;
    // `scripted_cleanup` only knows the *current* `engine_session_id` (the
    // successor) — the retired source's log, which is where the child's records
    // live, needs its own cleanup.
    let _ = site::ai::persistence::delete_session_events(&fx.db, &source_session_id).await;
    let _ = llm_provider::Entity::delete_by_id(provider_id)
        .exec(&fx.db)
        .await;
}
