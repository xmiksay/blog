//! The assistant-turn accumulator behind [`super::fold`] — buffered state for
//! the turn currently being folded, plus the one retroactive pass over the
//! finished message list that `flush_into` can't do on its own. Split out of
//! `mod.rs` to keep that file under the project's 400-line cap.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::ProjectedMessage;

/// Buffered state for the assistant turn currently being folded — reset by
/// [`OpenTurn::flush_into`], which is a no-op if nothing has accumulated.
#[derive(Default)]
pub(super) struct OpenTurn {
    pub(super) open: bool,
    pub(super) text: String,
    /// Accumulated `OutEvent::ReasoningDelta` text (#98). Display-only: it is
    /// surfaced in the transcript but never replayed to a provider — see the
    /// module-level note on [`super`].
    pub(super) reasoning: String,
    pub(super) tool_calls: Vec<Value>,
    pub(super) pending: HashSet<String>,
    pub(super) decisions: Vec<Value>,
}

impl OpenTurn {
    pub(super) fn flush_into(&mut self, out: &mut Vec<ProjectedMessage>) {
        if !self.open {
            return;
        }
        // Per-call, not just the message-level `requires_approval` below: a
        // batch can freely mix a call that actually paused for approval
        // (present in `self.pending`, i.e. it got its own `ToolRequest`) with
        // one the policy auto-allowed (only ever got a `ToolCall`, the
        // display-only event every call gets regardless). Both end up in
        // `tool_calls` either way, but only the former should ever offer an
        // Allow/Reject prompt — flagging the message as a whole isn't
        // specific enough for the client to tell them apart (see
        // `mark_resolved_calls`'s doc for the concrete symptom this caused).
        for tc in self.tool_calls.iter_mut() {
            let is_pending = tc
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| self.pending.contains(id));
            if is_pending && let Some(obj) = tc.as_object_mut() {
                obj.insert("requires_approval".into(), Value::Bool(true));
            }
        }
        let mut content = json!({
            "text": if self.text.is_empty() { Value::Null } else { json!(self.text) },
            "tool_calls": self.tool_calls,
        });
        if let Some(obj) = content.as_object_mut() {
            // Omitted entirely when empty — the vast majority of turns have no
            // reasoning at all, and every consumer predating #98 has to keep
            // working against a message that simply doesn't carry the key.
            if !self.reasoning.is_empty() {
                obj.insert("reasoning".into(), json!(self.reasoning));
            }
            if !self.pending.is_empty() {
                obj.insert("requires_approval".into(), Value::Bool(true));
            }
            if !self.decisions.is_empty() {
                obj.insert(
                    "decisions".into(),
                    Value::Array(std::mem::take(&mut self.decisions)),
                );
            }
        }
        out.push(ProjectedMessage {
            role: "assistant",
            content,
        });
        *self = OpenTurn::default();
    }
}

/// Retroactively flag every `tool_calls[]` entry that already has a matching
/// `tool_result` message as `"resolved": true` — the most robust signal
/// available for "is this call actually done", and one the client should
/// trust over `decisions` (below). [`OpenTurn::flush_into`]'s own per-call
/// `requires_approval` (paired with this) says whether a call was *ever*
/// gated at all; this says whether it's *still* worth a prompt. A client
/// should only ever offer Allow/Reject for a call with `requires_approval:
/// true` and no `resolved: true` — anything else is stale by construction.
///
/// Why this can't be derived from `decisions` alone: `InMsg::Approve`/
/// `Reject` is recorded into whichever `OpenTurn` happens to be accumulating
/// *at the moment that record is folded* — but a batch flushes (see
/// `fold`'s `ToolOutput` match arm) the instant its *first* call resolves,
/// before every sibling in the same batch is necessarily decided. A
/// second/third decision for that same already-flushed message, arriving
/// after the reset, lands in a fresh `OpenTurn` instead — silently orphaned
/// from the message it was actually deciding. Presence of a `tool_result` for
/// the same `tool_call_id` sidesteps this entirely: it's only ever emitted
/// once a call has genuinely resolved, regardless of how or when its
/// decision got folded.
pub(super) fn mark_resolved_calls(out: &mut [ProjectedMessage]) {
    let resolved: HashSet<String> = out
        .iter()
        .filter(|m| m.role == "tool_result")
        .filter_map(|m| m.content.get("tool_call_id")?.as_str().map(String::from))
        .collect();
    for msg in out.iter_mut() {
        if msg.role != "assistant" {
            continue;
        }
        let Some(tool_calls) = msg
            .content
            .get_mut("tool_calls")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for tc in tool_calls.iter_mut() {
            let is_resolved = tc
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| resolved.contains(id));
            if is_resolved && let Some(obj) = tc.as_object_mut() {
                obj.insert("resolved".into(), Value::Bool(true));
            }
        }
    }
}
