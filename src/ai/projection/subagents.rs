//! Sub-agent (#17) reference cards: attach a compact pointer to a spawned
//! child session on the specific `agent_spawn`/`agent` call that produced it.
//! Split out of `mod.rs` to keep that file under the project's 400-line cap.
//!
//! `assistant_events` rows for a whole session tree (a root plus any
//! `researcher`/`page-writer` children it spawned) all share one
//! `root_session_id` — see `persistence.rs` — so `project`'s `records` can
//! contain several sessions' worth of interleaved rows. `project` folds the
//! one session it was asked for and passes every *other* session's records
//! here.
//!
//! ## Card, not nested transcript (#100)
//!
//! Until #99 a child existed only inside its parent's log, so the only place
//! its transcript could be shown was nested under the spawning message —
//! `content.sub_agents[].messages`, rendered by a self-recursive Vue
//! component. That had two defects: a *grandchild* matched no call in the
//! root's own fold and was swept into a `role: "sub_agents"` trailing message
//! the client has no branch for (so it rendered as nothing at all), and a
//! research-heavy root carried every descendant's full transcript in one
//! response. Now that every session is an `assistant_sessions` row of its
//! own, the parent keeps only a card — `{agent_id, profile, task,
//! message_count, preview}` — and the transcript is read by opening the child
//! by its db id. Nothing is dropped by leaving an unmatched child cardless:
//! it is still a row, still in the session tree, still readable.
//!
//! `preview` (the child's last assistant text) and `message_count` are what
//! keep the parent transcript readable *without* that click — a card saying
//! only "researcher" would force a navigation to learn whether the child
//! answered at all.
//!
//! ## Structural matching
//!
//! Matching a child to its spawning call is **structural, not positional**:
//! `InMsg::Spawn` is never persisted (see `entanglement_runtime::persistence`'s
//! doc) and `OutEvent::SessionStarted` names the child's *parent session* but
//! not the tool call that produced it — but
//! `entanglement_runtime::subagent::launch`'s own immediate reply *text*
//! always names the child (`"...agent_id: {uuid}..."` for a detached
//! `agent_spawn`, `` "sub-agent `{uuid}` completed..." `` for a blocking
//! `agent`; re-verified unchanged against 0.6, `subagent.rs:396-404,434`), and
//! that reply is exactly the `tool_result` paired with *that* call's own
//! `tool_call_id`. [`extract_child_session_id`] recovers the uuid from it, so
//! matching only ever considers a call's own result — never an earlier or
//! later message's — and correctly handles both a spawn that never actually
//! started a session (a refusal's text contains no valid uuid, so it's skipped
//! rather than stealing a later real child) and two spawns in the same batch
//! racing to start concurrently (each still names its own child, so log order
//! between them is irrelevant). A child's profile name comes from its own
//! `SessionStarted` record; its task/prompt comes from the spawning call's own
//! `args.prompt`, so the client never has to re-derive either by position.

use std::collections::HashMap;

use entanglement_core::SessionId;
use entanglement_runtime::session_store::LogRecord;
use serde_json::{Value, json};

use super::{ProjectedMessage, fold};

/// Longest `preview` this emits, in characters. Long enough for a one-line
/// answer or the opening sentence of a longer one; short enough that a root
/// with a dozen children doesn't ship a second transcript in disguise.
const PREVIEW_CHARS: usize = 200;

/// Attach each spawned sub-agent's reference card to the specific
/// `agent_spawn`/`agent` call that produced it — see the module doc for why
/// this is a structural match (via [`extract_child_session_id`]), not a
/// positional one, and why it is a card rather than the child's transcript.
///
/// A child whose owning call isn't in `out` gets no card: that is either a
/// grandchild (whose spawning call lives in *its own* parent's transcript, not
/// this one) or a gap-truncated resume that lost the spawning message. Either
/// way the child is its own session row and is reached from the session tree.
pub(super) fn attach_sub_agents(
    // A slice, not a `Vec`: cards are attached in place. Nothing is appended
    // anymore now that the `role: "sub_agents"` leftover bucket is gone.
    out: &mut [ProjectedMessage],
    mut child_records: HashMap<SessionId, Vec<&LogRecord>>,
    child_profiles: &HashMap<SessionId, String>,
) {
    // Every tool_call_id's own result text, so a spawn call's match is scoped
    // to *its* result regardless of how far away it landed in `out`. Owned
    // (not borrowed) so the loop below can mutate `out` at the same time.
    let outputs: HashMap<String, String> = out
        .iter()
        .filter(|m| m.role == "tool_result")
        .filter_map(|m| {
            Some((
                m.content.get("tool_call_id")?.as_str()?.to_string(),
                m.content.get("output")?.as_str()?.to_string(),
            ))
        })
        .collect();

    for msg in out.iter_mut() {
        if msg.role != "assistant" {
            continue;
        }
        let Some(tool_calls) = msg
            .content
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
        else {
            continue;
        };
        let mut sub_agents = Vec::new();
        for tc in &tool_calls {
            let is_spawn = matches!(
                tc.get("name").and_then(Value::as_str),
                Some("agent_spawn" | "agent")
            );
            if !is_spawn {
                continue;
            }
            let Some(call_id) = tc.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(child_id) = outputs
                .get(call_id)
                .and_then(|output| extract_child_session_id(output))
            else {
                continue;
            };
            let child = SessionId::new(child_id);
            // Removed, not borrowed: one child belongs to exactly one call, so
            // a second call naming the same uuid can never duplicate its card.
            let Some(recs) = child_records.remove(&child) else {
                continue;
            };
            let task = tc
                .get("args")
                .and_then(|a| a.get("prompt"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            sub_agents.push(sub_agent_card(&child, child_profiles, task, &fold(&recs)));
        }
        if !sub_agents.is_empty()
            && let Some(obj) = msg.content.as_object_mut()
        {
            obj.insert("sub_agents".into(), Value::Array(sub_agents));
        }
    }
}

/// The card the parent transcript carries in place of the child's messages.
/// `child_db_session_id` — the row to open — is spliced on later by
/// `handlers::sessions::subagent_links`, which keeps this fold pure (and so
/// unit-testable without a database).
fn sub_agent_card(
    child: &SessionId,
    child_profiles: &HashMap<SessionId, String>,
    task: &str,
    messages: &[ProjectedMessage],
) -> Value {
    json!({
        "agent_id": child.0,
        "profile": child_profiles.get(child).cloned().unwrap_or_default(),
        "task": task,
        // Counts the child's whole projected transcript, every role included —
        // the same number its own view renders, so the two can't disagree.
        "message_count": messages.len(),
        "preview": preview(messages),
    })
}

/// The child's last assistant text, truncated to [`PREVIEW_CHARS`]. Empty
/// (never absent — the key's type stays stable) while the child has only
/// thought or called tools, which is exactly the still-running case where
/// there is genuinely nothing to preview yet.
fn preview(messages: &[ProjectedMessage]) -> String {
    let last = messages
        .iter()
        .rev()
        .filter(|m| m.role == "assistant")
        .find_map(|m| m.content.get("text").and_then(Value::as_str))
        .unwrap_or_default()
        .trim();
    match last.char_indices().nth(PREVIEW_CHARS) {
        // Char boundary, not byte: a multi-byte grapheme mid-cut would panic
        // on a byte slice.
        Some((cut, _)) => format!("{}…", &last[..cut]),
        None => last.to_string(),
    }
}

/// Recover a sub-agent child's own `SessionId` from its spawning call's
/// `tool_result` text — see the module doc. `entanglement_runtime::subagent::
/// launch`'s reply always embeds the child's raw uuid as one whitespace/
/// punctuation-delimited token (`` `{uuid}` `` or `agent_id: {uuid}.`);
/// scanning for the first token that parses as a uuid finds it regardless of
/// which of the two reply templates (detached `agent_spawn` vs blocking
/// `agent`) produced the text. A refusal's text (no valid uuid anywhere)
/// correctly yields `None` — nothing to match, not a wrong match.
///
/// Deliberately deferred (issue #28): `entanglement_runtime::subagent::launch`
/// has no structured field naming the child session either, and this crate is
/// a versioned dependency (not vendored in this repo), so there is nothing to
/// change here yet. Replace with a structural field the day `launch` grows
/// one.
fn extract_child_session_id(output: &str) -> Option<String> {
    output
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .find_map(|tok| uuid::Uuid::parse_str(tok).ok().map(|_| tok.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(text: &str) -> ProjectedMessage {
        ProjectedMessage {
            role: "assistant",
            content: json!({ "text": text, "tool_calls": [] }),
        }
    }

    /// The *last* assistant text wins — a child's earlier "working on it"
    /// round must not be what the parent card shows once it has answered.
    #[test]
    fn preview_takes_the_last_assistant_text() {
        let messages = vec![
            assistant("Looking into it."),
            ProjectedMessage {
                role: "tool_result",
                content: json!({ "tool_call_id": "c1", "output": "raw" }),
            },
            assistant("The answer is 4."),
        ];
        assert_eq!(preview(&messages), "The answer is 4.");
    }

    /// A child that has only called tools so far has nothing to preview — the
    /// key still has to be a string, not absent.
    #[test]
    fn preview_is_empty_while_the_child_has_not_spoken() {
        let messages = vec![ProjectedMessage {
            role: "assistant",
            content: json!({ "text": Value::Null, "tool_calls": [{ "id": "c1" }] }),
        }];
        assert_eq!(preview(&messages), "");
    }

    /// Truncation counts characters and cuts on a char boundary — slicing a
    /// multi-byte grapheme by byte offset would panic.
    #[test]
    fn preview_truncates_on_a_char_boundary() {
        let text = "é".repeat(PREVIEW_CHARS + 10);
        let out = preview(&[assistant(&text)]);
        assert_eq!(out.chars().count(), PREVIEW_CHARS + 1, "{out}");
        assert!(out.ends_with('…'));
    }
}
