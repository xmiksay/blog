//! Plain single-session projection shapes — text coalescing, error events
//! and round-boundary handling. Approval/resolution scenarios live in
//! `approval.rs`, reasoning (#98) in `reasoning.rs`, sub-agent (#17) nesting
//! in `subagent.rs`.

use super::*;

/// prompt -> assistant text-only turn.
#[test]
fn text_only_turn_projects_user_then_assistant() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            inm(InMsg::Prompt {
                session: s.clone(),
                content: vec![ContentPart::text("hi")],
            }),
        ),
        rec(
            &s,
            out(OutEvent::TextDelta {
                session: s.clone(),
                seq: 1,
                text: "Hello".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::TextDelta {
                session: s.clone(),
                seq: 2,
                text: " there".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::Done {
                session: s.clone(),
                seq: 3,
            }),
        ),
    ];

    let projected = project(&records);
    assert_eq!(
        projected,
        vec![
            ProjectedMessage {
                role: "user",
                content: json!({ "text": "hi" })
            },
            ProjectedMessage {
                role: "assistant",
                content: json!({ "text": "Hello there", "tool_calls": [] }),
            },
        ]
    );
}

#[test]
fn tool_error_output_is_flagged() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            out(OutEvent::ToolCall {
                session: s.clone(),
                seq: 1,
                request_id: "call-2".into(),
                tool: "bash".into(),
                input: "{}".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::ToolOutput {
                session: s.clone(),
                seq: 2,
                request_id: "call-2".into(),
                tool: "bash".into(),
                output: "tool `bash` denied by permission profile".into(),
                content: vec![],
            }),
        ),
    ];
    let projected = project(&records);
    let tool_result = &projected[1];
    assert_eq!(tool_result.role, "tool_result");
    assert_eq!(tool_result.content["is_error"], json!(true));
}

#[test]
fn error_event_flushes_open_turn_and_projects_error_message() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            out(OutEvent::TextDelta {
                session: s.clone(),
                seq: 1,
                text: "partial".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::Error {
                session: s.clone(),
                seq: 2,
                message: "boom".into(),
            }),
        ),
    ];
    let projected = project(&records);
    assert_eq!(
        projected,
        vec![
            ProjectedMessage {
                role: "assistant",
                content: json!({ "text": "partial", "tool_calls": [] }),
            },
            ProjectedMessage {
                role: "error",
                content: json!({ "text": "boom" })
            },
        ]
    );
}

/// An in-place ambiguous-stop retry (entanglement-core 0.4, ADR-0118) must not
/// corrupt turn folding: `AmbiguousRetry` falls into `project`'s `_ => {}` arm
/// (the projected transcript has no concept of a "retry", only the final
/// text), so the pre- and post-retry `TextDelta`s coalesce into one assistant
/// message and the synthetic `nudge` never surfaces as its own message.
#[test]
fn ambiguous_retry_is_ignored_and_text_deltas_around_it_still_coalesce() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            out(OutEvent::TextDelta {
                session: s.clone(),
                seq: 1,
                text: "partial answer".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::AmbiguousRetry {
                session: s.clone(),
                seq: 2,
                nudge: "please continue or call a tool".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::TextDelta {
                session: s.clone(),
                seq: 3,
                text: " continued".into(),
            }),
        ),
        rec(
            &s,
            out(OutEvent::Done {
                session: s.clone(),
                seq: 4,
            }),
        ),
    ];

    let projected = project(&records);
    assert_eq!(
        projected,
        vec![ProjectedMessage {
            role: "assistant",
            content: json!({ "text": "partial answer continued", "tool_calls": [] }),
        }]
    );
}
