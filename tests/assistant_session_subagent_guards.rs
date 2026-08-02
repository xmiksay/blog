//! #101: what a sub-agent (#99) `assistant_sessions` row does and does not
//! accept.
//!
//! **Refused (`409`)** — `PATCH`, `compact`, `DELETE`. Each would corrupt the
//! tree in its own way (a child can't take a model/profile switch, a compaction
//! successor is root-shaped, and deleting only the child's row strands events
//! filed under the root). But the assertion that actually matters here is the
//! *second* one in each case: `GET /sessions/{root}` afterwards must still
//! return the whole transcript. Every one of those handlers used to
//! `ensure_live` the row's own `engine_session_id`, which for a child resolves
//! to zero `assistant_events` rows — resuming it would materialize a blank
//! engine session under that id and cache it as live, poisoning it for good.
//! A still-complete root transcript is what proves no such resume happened.
//!
//! **Allowed** — `GET` and `POST .../messages`. A child is a first-class,
//! readable and prompt-able session: upstream never closes a finished
//! sub-agent, so it stays live and can simply be prompted, and its transcript
//! is projected out of the root's shared log by its own engine id.
//!
//! DB-gated and driven by a scripted `Llm`, like the sibling
//! `tests/assistant_session_subagent_rows.rs` — see that file's doc.

mod common;
#[path = "common/scripted.rs"]
mod scripted;

use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::{send, test_db_url};
use entanglement_core::{Llm, LlmRequest, LlmResponse, LlmStream, ToolCall, stream_from_response};
use scripted::{ScriptedFixture, scripted_cleanup, scripted_session, setup_scripted};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use site::entity::assistant_session;

/// The child's answer to its spawn prompt, and the marker a follow-up turn on
/// the child must still be able to see in its own context.
const CHILD_ANSWER: &str = "2 + 2 = 4.";
const FOLLOWUP: &str = "and 3+3?";

/// Root spawns one `researcher`; the child answers its spawn prompt, and
/// answers a later direct prompt differently so the two turns are
/// distinguishable in the transcript.
#[derive(Default)]
struct GuardScriptedLlm {
    calls: u32,
}

#[async_trait]
impl Llm for GuardScriptedLlm {
    async fn stream(&mut self, req: LlmRequest<'_>) -> anyhow::Result<LlmStream> {
        self.calls += 1;
        let history: String = req
            .messages
            .iter()
            .map(|m| m.text())
            .collect::<Vec<_>>()
            .join("\n");
        let resp = if history.contains(FOLLOWUP) {
            // Echo back whether the prior turn is still in context: a blank
            // resume would have wiped it, and this text lands in the response
            // the test asserts on.
            LlmResponse {
                text: format!("3 + 3 = 6. (remembered={})", history.contains(CHILD_ANSWER)),
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
                text: "Researching, thanks!".into(),
                tool_calls: vec![],
            }
        };
        Ok(stream_from_response(resp))
    }
}

fn guard_factory() -> entanglement_core::LlmFactory {
    std::sync::Arc::new(|| Box::new(GuardScriptedLlm::default()))
}

/// Drive one turn on the root that spawns a `researcher`, then wait for the
/// child's own row. Returns `(root row id, child row)`.
async fn spawn_child(fx: &ScriptedFixture, tag: &str) -> (i32, assistant_session::Model) {
    let (root_id, _root_session) = scripted_session(fx).await;
    let (status, resp) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{root_id}/messages"),
        &fx.cookie,
        Some(json!({ "text": "research 2+2" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tag}: send_message: {resp}");

    // The child runs detached (ADR-0026) and both row writers are asynchronous
    // with respect to this request, so poll — re-reading the root each time,
    // which is also what drives the handler-side hydration writer.
    for _ in 0..40 {
        if let Some(row) = assistant_session::Entity::find()
            .filter(assistant_session::Column::ParentSessionId.eq(root_id))
            .one(&fx.db)
            .await
            .expect("query assistant_sessions")
        {
            return (root_id, row);
        }
        let _ = send(
            &fx.app,
            "GET",
            &format!("/assistant/sessions/{root_id}"),
            &fx.cookie,
            None,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{tag}: the spawned sub-agent never got its own assistant_sessions row");
}

/// `GET /sessions/{child_id}` once the child's own answer has landed in the
/// log. `DbSink` persists asynchronously, so the child's row can exist a beat
/// before the records its transcript is projected from.
async fn await_child_transcript(fx: &ScriptedFixture, child_id: i32) -> Value {
    let mut last = Value::Null;
    for _ in 0..40 {
        let (status, view) = send(
            &fx.app,
            "GET",
            &format!("/assistant/sessions/{child_id}"),
            &fx.cookie,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "read child: {view}");
        if texts(&view).iter().any(|t| t.contains(CHILD_ANSWER)) {
            return view;
        }
        last = view;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the sub-agent's own answer never showed up in its transcript: {last:#}")
}

fn texts(detail: &Value) -> Vec<String> {
    detail["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["content"]["text"].as_str().map(String::from))
        .collect()
}

/// The root's transcript is intact: its own prompt is still there and nothing
/// projected as an error. Called after each refused operation — a blank resume
/// of the child would not corrupt the *root's* log directly, but every one of
/// these handlers reached the engine through the child's id, and a session that
/// answers with a full history is the observable proof it never got there.
async fn assert_root_transcript_intact(fx: &ScriptedFixture, root_id: i32, tag: &str) {
    let (status, resp) = send(
        &fx.app,
        "GET",
        &format!("/assistant/sessions/{root_id}"),
        &fx.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tag}: read root: {resp}");
    assert!(
        texts(&resp).iter().any(|t| t.contains("research 2+2")),
        "{tag}: the root's own transcript went missing: {resp:#}"
    );
    assert!(
        !resp["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|m| m["role"] == "error"),
        "{tag}: the root's transcript now contains an error: {resp:#}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn update_compact_and_delete_are_refused_on_a_sub_agent_row() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted(&db_url, "guards-refuse", guard_factory()).await;
    let (root_id, child) = spawn_child(&fx, "refuse").await;

    let (status, resp) = send(
        &fx.app,
        "PATCH",
        &format!("/assistant/sessions/{}", child.id),
        &fx.cookie,
        Some(json!({ "title": "renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "PATCH on a child: {resp}");
    assert_root_transcript_intact(&fx, root_id, "after PATCH").await;

    let (status, resp) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{}/compact", child.id),
        &fx.cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "compact on a child: {resp}");
    assert_root_transcript_intact(&fx, root_id, "after compact").await;

    let (status, resp) = send(
        &fx.app,
        "DELETE",
        &format!("/assistant/sessions/{}", child.id),
        &fx.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "DELETE on a child: {resp}");
    assert_root_transcript_intact(&fx, root_id, "after DELETE").await;

    // The refusal is real, not cosmetic: the row is still there.
    assert!(
        assistant_session::Entity::find_by_id(child.id)
            .one(&fx.db)
            .await
            .expect("query assistant_sessions")
            .is_some(),
        "a refused DELETE must leave the child row in place"
    );

    scripted_cleanup(&fx, root_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sub_agent_row_can_be_read_and_prompted() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted(&db_url, "guards-interactive", guard_factory()).await;
    let (root_id, child) = spawn_child(&fx, "interactive").await;

    // Read: the child's *own* turn, folded out of the root's shared log by its
    // own engine id — not the root's transcript, and not an empty one. The
    // child's row can exist before its turn's records do (`DbSink` appends
    // behind its own writer task), so poll for the answer rather than assuming
    // the row's appearance means the log has caught up.
    let view = await_child_transcript(&fx, child.id).await;
    let read_texts = texts(&view);
    assert!(
        read_texts.iter().any(|t| t.contains("what is 2+2")),
        "the child's spawn prompt should open its transcript: {view:#}"
    );
    assert!(
        read_texts.iter().any(|t| t.contains(CHILD_ANSWER)),
        "the child's own answer should be in its transcript: {view:#}"
    );
    assert!(
        !read_texts.iter().any(|t| t.contains("research 2+2")),
        "the root's prompt must not leak into the child's transcript: {view:#}"
    );

    // Prompt: the child takes a direct follow-up, and answers it with its own
    // prior turn still in context.
    let (status, turn) = send(
        &fx.app,
        "POST",
        &format!("/assistant/sessions/{}/messages", child.id),
        &fx.cookie,
        Some(json!({ "text": FOLLOWUP })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "prompt child: {turn}");
    let turn_texts = texts(&turn);
    assert!(
        turn_texts.iter().any(|t| t.contains(FOLLOWUP)),
        "the follow-up prompt should be in the response: {turn:#}"
    );
    assert!(
        turn_texts.iter().any(|t| t.contains("remembered=true")),
        "the child's follow-up turn ran without its own prior context — that is \
         the blank-resume failure this issue exists to prevent: {turn:#}"
    );
    assert!(
        !turn["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|m| m["role"] == "error"),
        "the child's follow-up turn errored: {turn:#}"
    );

    scripted_cleanup(&fx, root_id).await;
}

/// Deleting the **root** is allowed and takes the whole tree with it: the rows
/// go by the self-FK cascade, and the handler retires each descendant's engine
/// session on the way so nothing is left pointing at a row that no longer
/// exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deleting_the_root_removes_its_sub_agent_rows_too() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted(&db_url, "guards-delete-root", guard_factory()).await;
    let (root_id, child) = spawn_child(&fx, "delete-root").await;

    let (status, resp) = send(
        &fx.app,
        "DELETE",
        &format!("/assistant/sessions/{root_id}"),
        &fx.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "DELETE root: {resp}");

    assert!(
        assistant_session::Entity::find_by_id(child.id)
            .one(&fx.db)
            .await
            .expect("query assistant_sessions")
            .is_none(),
        "the child row must go with its root"
    );

    scripted_cleanup(&fx, root_id).await;
}

/// `list` stays a flat array, but ordered so the client can render it top to
/// bottom as a tree: a child is created *during* its parent's turn, so a naive
/// `updated_at DESC` would sort it above its own parent.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn list_orders_a_child_after_its_own_parent() {
    let Some(db_url) = test_db_url().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let fx = setup_scripted(&db_url, "guards-list", guard_factory()).await;
    let (root_id, child) = spawn_child(&fx, "list").await;

    let (status, list) = send(&fx.app, "GET", "/assistant/sessions", &fx.cookie, None).await;
    assert_eq!(status, StatusCode::OK, "list: {list}");
    let ids: Vec<i64> = list
        .as_array()
        .expect("list is an array")
        .iter()
        .filter_map(|r| r["id"].as_i64())
        .collect();
    let root_at = ids
        .iter()
        .position(|&i| i == root_id as i64)
        .unwrap_or_else(|| panic!("root missing from the list: {list:#}"));
    let child_at = ids
        .iter()
        .position(|&i| i == child.id as i64)
        .unwrap_or_else(|| panic!("child missing from the list: {list:#}"));
    assert_eq!(
        child_at,
        root_at + 1,
        "the child must follow its own parent immediately: {list:#}"
    );

    scripted_cleanup(&fx, root_id).await;
}
