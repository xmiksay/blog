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
