//! Reasoning/thinking folding (#98) — `OutEvent::ReasoningDelta` was always
//! persisted but dropped by `fold`'s catch-all; these pin the shape it now
//! takes in the transcript.

use super::*;

/// Consecutive `ReasoningDelta`s coalesce into one `reasoning` string on the
/// same assistant message the turn's text lands on.
#[test]
fn reasoning_deltas_fold_onto_the_assistant_message() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            inm(InMsg::Prompt {
                session: s.clone(),
                content: vec![ContentPart::text("2+2?")],
            }),
        ),
        reasoning(&s, 1, "Let me "),
        reasoning(&s, 2, "add them."),
        text_delta(&s, 3, "4"),
        done(&s, 4),
    ];

    let projected = project(&records, &s);
    assert_eq!(
        projected,
        vec![
            ProjectedMessage {
                role: "user",
                content: json!({ "text": "2+2?" }),
            },
            ProjectedMessage {
                role: "assistant",
                content: json!({
                    "text": "4",
                    "reasoning": "Let me add them.",
                    "tool_calls": [],
                }),
            },
        ]
    );
}

/// A turn with no reasoning must not grow the key at all — consumers
/// predating #98 see a byte-identical message.
#[test]
fn a_turn_without_reasoning_omits_the_key_entirely() {
    let s = SessionId::new("u1:test");
    let records = vec![text_delta(&s, 1, "hi"), done(&s, 2)];

    let projected = project(&records, &s);
    assert_eq!(projected.len(), 1);
    assert!(
        projected[0].content.get("reasoning").is_none(),
        "reasoning must be absent, not null/empty: {projected:#?}"
    );
}

/// Reasoning, text and a tool call in the same round all land on one message.
#[test]
fn reasoning_text_and_tool_calls_share_one_assistant_message() {
    let s = SessionId::new("u1:test");
    let records = vec![
        reasoning(&s, 1, "I should look this up."),
        text_delta(&s, 2, "Searching now."),
        tool_call(&s, 3, "call-1", "page_search"),
        tool_output(&s, 4, "call-1", "page_search", "one match"),
    ];

    let projected = project(&records, &s);
    assert_eq!(
        projected[0],
        ProjectedMessage {
            role: "assistant",
            content: json!({
                "text": "Searching now.",
                "reasoning": "I should look this up.",
                "tool_calls": [{
                    "id": "call-1",
                    "name": "page_search",
                    "args": {},
                    "resolved": true,
                }],
            }),
        }
    );
}

/// A round that only ever thought — no text, no tool calls — still produces a
/// message, since `ReasoningDelta` opens the turn on its own. Without that,
/// the reasoning would be dropped at the flush.
#[test]
fn a_reasoning_only_turn_still_emits_an_assistant_message() {
    let s = SessionId::new("u1:test");
    let records = vec![reasoning(&s, 1, "Hmm, nothing to do."), done(&s, 2)];

    let projected = project(&records, &s);
    assert_eq!(
        projected,
        vec![ProjectedMessage {
            role: "assistant",
            content: json!({
                "text": Value::Null,
                "reasoning": "Hmm, nothing to do.",
                "tool_calls": [],
            }),
        }]
    );
}

/// Two tool rounds inside one prompt: `flush_into` fires on each `ToolOutput`,
/// so each round's reasoning is attributed to *its own* assistant message
/// rather than all of it piling onto the last one. This is the property that
/// makes per-round thinking readable in the transcript at all.
#[test]
fn two_tool_rounds_split_reasoning_across_two_assistant_messages() {
    let s = SessionId::new("u1:test");
    let records = vec![
        rec(
            &s,
            inm(InMsg::Prompt {
                session: s.clone(),
                content: vec![ContentPart::text("fix the page")],
            }),
        ),
        reasoning(&s, 1, "First, find it."),
        tool_call(&s, 2, "call-1", "page_search"),
        tool_output(&s, 3, "call-1", "page_search", "found: foo"),
        reasoning(&s, 4, "Now edit it."),
        tool_call(&s, 5, "call-2", "page_edit"),
        tool_output(&s, 6, "call-2", "page_edit", "updated: foo"),
        text_delta(&s, 7, "Fixed."),
        done(&s, 8),
    ];

    let projected = project(&records, &s);
    let roles: Vec<&str> = projected.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            "user",
            "assistant",
            "tool_result",
            "assistant",
            "tool_result",
            "assistant",
        ]
    );
    assert_eq!(projected[1].content["reasoning"], json!("First, find it."));
    assert_eq!(projected[3].content["reasoning"], json!("Now edit it."));
    assert!(
        projected[5].content.get("reasoning").is_none(),
        "the final text-only round thought nothing: {projected:#?}"
    );
}

/// A sub-agent child's own reasoning belongs to the child's own transcript,
/// never the root's: reasoning is folded per session, so the root's
/// projection must carry none of it — not even by way of the card, which is a
/// pointer and has no reasoning field at all.
#[test]
fn sub_agent_reasoning_lands_on_the_childs_own_transcript() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new("3fa85f64-5717-4562-b3fc-2c963f66afa6");
    let records = vec![
        tool_call(&root, 1, "spawn-1", "agent_spawn"),
        session_started(&child, &root, "researcher"),
        reasoning(&child, 1, "The child is thinking."),
        text_delta(&child, 2, "Answer."),
        done(&child, 3),
        tool_output(&root, 2, "spawn-1", "agent_spawn", &spawn_reply(&child)),
        done(&root, 3),
    ];

    let from_child = project(&records, &child);
    assert_eq!(
        from_child[0].content,
        json!({
            "text": "Answer.",
            "reasoning": "The child is thinking.",
            "tool_calls": [],
        })
    );

    let from_root = project(&records, &root);
    assert!(
        from_root[0].content["sub_agents"][0]
            .get("reasoning")
            .is_none(),
        "a card is a pointer, it carries no thinking: {from_root:#?}"
    );
    assert!(
        from_root[0].content.get("reasoning").is_none(),
        "the root never thought — the child's reasoning must not leak up: {from_root:#?}"
    );
}
