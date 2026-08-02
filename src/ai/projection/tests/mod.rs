//! Unit tests for [`super::project`], split by scenario: plain single-session
//! turn shapes live in `basic.rs`, tool-approval/resolution in `approval.rs`,
//! reasoning folding (#98) in `reasoning.rs`, and sub-agent (#17) nesting in
//! `subagent.rs`. Shared record-building helpers live here.

use super::*;
use entanglement_core::SessionId;
use entanglement_provider::ContentPart;

mod approval;
mod basic;
mod reasoning;
mod subagent;

fn rec(session: &SessionId, payload: LogPayload) -> LogRecord {
    LogRecord {
        ts: 0,
        session: session.clone(),
        payload,
    }
}

fn out(ev: OutEvent) -> LogPayload {
    LogPayload::Out(ev)
}
fn inm(msg: InMsg) -> LogPayload {
    LogPayload::In(msg)
}

fn prompt(s: &SessionId, text: &str) -> LogRecord {
    rec(
        s,
        inm(InMsg::Prompt {
            session: s.clone(),
            content: vec![ContentPart::text(text)],
        }),
    )
}

fn reasoning(s: &SessionId, seq: u64, text: &str) -> LogRecord {
    rec(
        s,
        out(OutEvent::ReasoningDelta {
            session: s.clone(),
            seq,
            text: text.into(),
        }),
    )
}

fn text_delta(s: &SessionId, seq: u64, text: &str) -> LogRecord {
    rec(
        s,
        out(OutEvent::TextDelta {
            session: s.clone(),
            seq,
            text: text.into(),
        }),
    )
}

fn tool_call(s: &SessionId, seq: u64, id: &str, tool: &str) -> LogRecord {
    tool_call_with(s, seq, id, tool, "{}")
}

fn tool_call_with(s: &SessionId, seq: u64, id: &str, tool: &str, input: &str) -> LogRecord {
    rec(
        s,
        out(OutEvent::ToolCall {
            session: s.clone(),
            seq,
            request_id: id.into(),
            tool: tool.into(),
            input: input.into(),
        }),
    )
}

fn tool_output(s: &SessionId, seq: u64, id: &str, tool: &str, output: &str) -> LogRecord {
    rec(
        s,
        out(OutEvent::ToolOutput {
            session: s.clone(),
            seq,
            request_id: id.into(),
            tool: tool.into(),
            output: output.into(),
            content: vec![],
        }),
    )
}

fn done(s: &SessionId, seq: u64) -> LogRecord {
    rec(
        s,
        out(OutEvent::Done {
            session: s.clone(),
            seq,
        }),
    )
}

/// A spawned child's own `SessionStarted` — the record that names its profile
/// and its parent, and the only place either comes from.
fn session_started(child: &SessionId, parent: &SessionId, profile: &str) -> LogRecord {
    rec(
        child,
        out(OutEvent::SessionStarted {
            session: child.clone(),
            parent: Some(parent.clone()),
            predecessor: None,
            profile: profile.into(),
            model: None,
            user: None,
            root: false,
            ts: 0,
        }),
    )
}

/// `entanglement_runtime::subagent::launch`'s own reply text for a detached
/// `agent_spawn` — the only thing that links a spawning call to the child it
/// produced (see `super::subagents`).
fn spawn_reply(child: &SessionId) -> String {
    format!(
        "Sub-agent launched under the profile. agent_id: {}. \
         Call agent_poll with this agent_id to await its answer.",
        child.0
    )
}
