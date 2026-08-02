//! Fold a session's `assistant_events` rows (each a
//! `entanglement_runtime::session_store::LogRecord`) into the JSON shape the
//! Vue admin client already understands — `role` one of `user | assistant |
//! tool_result | error`, with the `content` shapes documented on [`project`].
//! Pure logic, no DB access — the easiest piece in this batch to unit test
//! (see `tests/`).
//!
//! **Scope note:** this returns `{"role", "content"}` pairs only, not a full
//! `MessageView` (`id`/`seq`/`created_at`). An `assistant_events` row doesn't
//! map 1:1 to a client-visible "message" the way an `assistant_messages` row
//! used to — several `TextDelta`s fold into one assistant message, and a
//! multi-tool-call batch's calls/results interleave across several rows — so
//! deciding stable synthetic ids/seqs for the wrapped `MessageView` is left to
//! the next phase, once it settles how `GET /api/assistant/sessions/{id}`
//! numbers these going forward.
//!
//! **`is_error` limitation:** `OutEvent::ToolOutput` carries no explicit error
//! flag (unlike the old system's `ToolRegistry::dispatch` which returned one
//! directly). `entanglement_runtime::tool_runner`'s own reply text for every
//! failure path (`Deny`, reject, spawn-mask, unknown tool, or the executor's
//! `tool `{name}` failed: {e}` wrap) always starts with `"tool `"` or
//! `"unknown tool:"` — [`looks_like_tool_error`] keys off that. Re-checked
//! against entanglement-core 0.4.0 (issue #87): `ToolOutput` still carries
//! only `output`/`content`, no error flag, so the heuristic stands. It's a
//! heuristic, not a structural guarantee; a future engine release exposing a
//! real flag on `ToolOutput` should replace it.
//!
//! ## Sub-agent (#17, #100) reference cards
//!
//! `assistant_events` rows for a whole session tree (a root plus any
//! `researcher`/`page-writer` children it spawned) all share one
//! `root_session_id` — so `records` here can contain several sessions' worth
//! of interleaved rows. [`project`] folds **one** of them (`target`) and
//! hands every other session's records to [`subagents`], which attaches a
//! *reference card* — profile, task, message count, preview, no nested
//! transcript — to the spawning tool call. See that module's doc for the
//! structural (not positional) child-to-spawning-call matching, and for why
//! nesting the child's whole transcript here was retired in #100.
//!
//! ## Reasoning (#98)
//!
//! `OutEvent::ReasoningDelta` is already persisted (the runtime's tap has no
//! allowlist), and folds into the enclosing assistant message's optional
//! `"reasoning"` string. It is **display-only in both directions**: the engine
//! never feeds it back to a provider — `entanglement-provider`'s Anthropic SSE
//! reader discards `signature_delta` (`anthropic/sse.rs:192-194`), so a
//! thinking block could not be replayed verifiably even if we wanted to, and
//! core's own context rebuild drops `ReasoningDelta` outright
//! (`session/replay.rs:132-134`). So this transcript field is for the reader,
//! not for the model.

#[cfg(test)]
mod tests;

mod subagents;
mod turn;

use std::collections::HashMap;

use entanglement_core::{InMsg, OutEvent, SessionId};
use entanglement_runtime::session_store::{LogPayload, LogRecord};
use serde_json::{Value, json};
use subagents::attach_sub_agents;
use turn::{OpenTurn, mark_resolved_calls};

/// One projected client-visible message: `{"role": ..., "content": ...}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedMessage {
    pub role: &'static str,
    pub content: Value,
}

/// Fold **one** session's ordered event log into projected messages, leaving a
/// reference card on the turn that spawned each of its sub-agent (#17)
/// children (see the module doc). Records must be in the order they were
/// appended (`assistant_events` ordered by `id`, i.e. insertion order) — this
/// is a linear fold, not a sort.
///
/// `records` is the whole session tree's log (every descendant's rows are
/// filed under one `root_session_id`); `target` picks which session in it is
/// being read. A `target` no record belongs to — an unknown id, or an empty
/// log — projects to an empty transcript, never a panic: a session row can
/// legitimately exist before its first event is persisted.
///
/// Every session is reachable by its own `assistant_sessions` row since #99,
/// so this deliberately does *not* recurse: a grandchild is a card on the
/// child's transcript, reached by opening the child, not a nested array here.
pub fn project(records: &[LogRecord], target: &SessionId) -> Vec<ProjectedMessage> {
    let mut own: Vec<&LogRecord> = Vec::new();
    let mut child_records: HashMap<SessionId, Vec<&LogRecord>> = HashMap::new();
    let mut child_profiles: HashMap<SessionId, String> = HashMap::new();
    for record in records {
        if &record.session == target {
            own.push(record);
            continue;
        }
        if let LogPayload::Out(OutEvent::SessionStarted { profile, .. }) = &record.payload {
            child_profiles.insert(record.session.clone(), profile.clone());
        }
        child_records
            .entry(record.session.clone())
            .or_default()
            .push(record);
    }

    let mut out = fold(&own);
    if !child_records.is_empty() {
        attach_sub_agents(&mut out, child_records, &child_profiles);
    }
    out
}

/// The original per-session fold, shared by [`project`] for the root's own
/// records and for each sub-agent child's own record slice.
fn fold(records: &[&LogRecord]) -> Vec<ProjectedMessage> {
    let mut out = Vec::new();
    let mut turn = OpenTurn::default();

    for record in records {
        match &record.payload {
            LogPayload::In(InMsg::Prompt { content, .. }) => {
                turn.flush_into(&mut out);
                let text = entanglement_core::content_text(content);
                out.push(ProjectedMessage {
                    role: "user",
                    content: json!({ "text": text }),
                });
            }
            LogPayload::In(InMsg::Approve { request_id, .. }) => {
                turn.decisions
                    .push(json!({ "tool_call_id": request_id, "approve": true }));
            }
            LogPayload::In(InMsg::Reject { request_id, .. }) => {
                turn.decisions
                    .push(json!({ "tool_call_id": request_id, "approve": false }));
            }
            LogPayload::Out(OutEvent::TextDelta { text, .. }) => {
                turn.open = true;
                turn.text.push_str(text);
            }
            LogPayload::Out(OutEvent::ReasoningDelta { text, .. }) => {
                // Opens the turn on its own: a round that only ever thinks and
                // then calls a tool still has to produce an assistant message,
                // or its reasoning would be silently dropped at the flush.
                turn.open = true;
                turn.reasoning.push_str(text);
            }
            LogPayload::Out(OutEvent::ToolCall {
                request_id,
                tool,
                input,
                ..
            }) => {
                turn.open = true;
                let args: Value = serde_json::from_str(input).unwrap_or_else(|_| json!(input));
                turn.tool_calls.push(json!({
                    "id": request_id,
                    "name": tool,
                    "args": args,
                }));
            }
            LogPayload::Out(OutEvent::ToolRequest { request_id, .. }) => {
                turn.open = true;
                turn.pending.insert(request_id.clone());
            }
            LogPayload::Out(OutEvent::ToolOutput {
                request_id, output, ..
            }) => {
                // A resolved call means this turn's tool_calls are fully
                // enumerated (core emits the whole batch before any result
                // comes back) — flush the assistant message before recording
                // the result, so the client sees them in the right order.
                turn.flush_into(&mut out);
                out.push(ProjectedMessage {
                    role: "tool_result",
                    content: json!({
                        "tool_call_id": request_id,
                        "output": output,
                        "is_error": looks_like_tool_error(output),
                    }),
                });
            }
            LogPayload::Out(OutEvent::Done { .. }) => {
                turn.flush_into(&mut out);
            }
            LogPayload::Out(OutEvent::Error { message, .. }) => {
                turn.flush_into(&mut out);
                out.push(ProjectedMessage {
                    role: "error",
                    content: json!({ "text": message }),
                });
            }
            _ => {} // lifecycle/status events carry no client-visible content
        }
    }
    turn.flush_into(&mut out);
    mark_resolved_calls(&mut out);
    out
}

/// `tool_runner`'s reply text for every failure path (`Deny`/reject/mask/
/// unknown-tool/execution-error) starts with one of these two prefixes — see
/// the module doc for why this is a heuristic, not a structural flag.
///
/// Deliberately deferred (originally issue #28, re-investigated for 0.3.0 by
/// #43): `OutEvent::ToolOutput` still carries no error flag to key off
/// instead, and this crate is a versioned dependency (not vendored in this
/// repo), so there is nothing to change here yet. Replace with a structural
/// flag the day `ToolOutput` grows one.
fn looks_like_tool_error(output: &str) -> bool {
    output.starts_with("tool `") || output.starts_with("unknown tool:")
}
