//! Sub-agent (#17, #100) reference-card scenarios — see `basic.rs` for the
//! plain, single-session projection shapes.
//!
//! The whole tree (root, child, grandchild) shares one log, so every test here
//! builds one `records` vec and projects it repeatedly with a different
//! `target` — which is the point of #100: what you see depends only on which
//! session you asked for.

use super::*;

const CHILD: &str = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
const GRANDCHILD: &str = "9c858901-8a57-4791-81fe-4c455b266ed5";

/// root ── agent_spawn ──▶ child ── agent_spawn ──▶ grandchild, each answering
/// with plain text. One log, three sessions, as `assistant_events` files them.
fn tree_records(root: &SessionId, child: &SessionId, grandchild: &SessionId) -> Vec<LogRecord> {
    vec![
        prompt(root, "research topic X"),
        tool_call_with(
            root,
            1,
            "spawn-1",
            "agent_spawn",
            r#"{"agent":"researcher","prompt":"look into X"}"#,
        ),
        session_started(child, root, "researcher"),
        prompt(child, "look into X"),
        tool_call_with(
            child,
            1,
            "spawn-2",
            "agent_spawn",
            r#"{"agent":"page-writer","prompt":"write it up"}"#,
        ),
        session_started(grandchild, child, "page-writer"),
        text_delta(grandchild, 1, "Written."),
        done(grandchild, 2),
        tool_output(child, 2, "spawn-2", "agent_spawn", &spawn_reply(grandchild)),
        text_delta(child, 3, "X is interesting."),
        done(child, 4),
        tool_output(root, 2, "spawn-1", "agent_spawn", &spawn_reply(child)),
        text_delta(root, 3, "Researching..."),
        done(root, 4),
    ]
}

fn cards(msg: &ProjectedMessage) -> Vec<Value> {
    msg.content["sub_agents"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The root's own transcript: its own messages only — the child's prompt/text
/// never leaks in — plus one card naming the child on the spawning turn.
#[test]
fn root_projects_its_own_messages_and_a_card_for_its_child() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new(CHILD);
    let grandchild = SessionId::new(GRANDCHILD);
    let records = tree_records(&root, &child, &grandchild);

    let projected = project(&records, &root);
    // user, the spawning turn, its tool_result, the closing text.
    assert_eq!(projected.len(), 4, "{projected:#?}");
    assert_eq!(projected[0].role, "user");

    let spawning_turn = &projected[1];
    assert_eq!(spawning_turn.role, "assistant");
    assert_eq!(
        cards(spawning_turn),
        vec![json!({
            "agent_id": child.0,
            "profile": "researcher",
            "task": "look into X",
            // The child's own user prompt, its spawning turn, that call's
            // tool_result and its closing text — its whole transcript, which
            // is what the child's own view renders.
            "message_count": 4,
            "preview": "X is interesting.",
        })],
        "{spawning_turn:#?}"
    );
    // The card is a pointer: no transcript rides along with it.
    assert!(cards(spawning_turn)[0].get("messages").is_none());

    assert_eq!(projected[2].role, "tool_result");
    assert_eq!(projected[3].content["text"], json!("Researching..."));
    assert!(projected[3].content.get("sub_agents").is_none());
}

/// The regression this issue exists for: a **grandchild** used to match no
/// call in the root's fold and was swept into a `role: "sub_agents"` trailing
/// message the client has no branch for — it rendered as nothing. Projected
/// from the child, it is an ordinary card on the child's own spawning turn.
#[test]
fn child_projects_its_own_messages_and_a_card_for_the_grandchild() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new(CHILD);
    let grandchild = SessionId::new(GRANDCHILD);
    let records = tree_records(&root, &child, &grandchild);

    let projected = project(&records, &child);
    assert_eq!(projected.len(), 4, "{projected:#?}");
    assert_eq!(projected[0].role, "user");
    assert_eq!(projected[0].content["text"], json!("look into X"));

    assert_eq!(
        cards(&projected[1]),
        vec![json!({
            "agent_id": grandchild.0,
            "profile": "page-writer",
            "task": "write it up",
            "message_count": 1,
            "preview": "Written.",
        })],
        "{projected:#?}"
    );
    assert_eq!(projected[3].content["text"], json!("X is interesting."));

    // The retired leftover bucket: no message may carry that role anymore.
    assert!(
        projected.iter().all(|m| m.role != "sub_agents"),
        "{projected:#?}"
    );
}

/// A leaf: its own messages, no card, and nothing from the two sessions above
/// it in the same log.
#[test]
fn grandchild_projects_its_own_messages_and_no_card() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new(CHILD);
    let grandchild = SessionId::new(GRANDCHILD);
    let records = tree_records(&root, &child, &grandchild);

    let projected = project(&records, &grandchild);
    assert_eq!(projected.len(), 1, "{projected:#?}");
    assert_eq!(projected[0].content["text"], json!("Written."));
    assert!(projected[0].content.get("sub_agents").is_none());
}

/// A target no record belongs to — a session row whose first event hasn't been
/// persisted yet, or an id from another tree entirely — is an empty
/// transcript, never a panic. Same for an empty log.
#[test]
fn an_unknown_or_empty_target_projects_nothing() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new(CHILD);
    let grandchild = SessionId::new(GRANDCHILD);
    let records = tree_records(&root, &child, &grandchild);

    assert!(project(&records, &SessionId::new("u1:nobody")).is_empty());
    assert!(project(&records, &SessionId::new("")).is_empty());
    assert!(project(&[], &root).is_empty());
}

/// A regression for count/position-based pairing: an earlier `agent_spawn`
/// call is *refused* (no child session ever starts — its tool_result names no
/// uuid) and a later, unrelated call actually spawns one. The real child must
/// card the call that actually produced it, never the refused one merely
/// because it came first in the log.
#[test]
fn a_refused_spawn_does_not_steal_a_later_calls_real_child() {
    let root = SessionId::new("u1:root");
    let child = SessionId::new(CHILD);
    let records = vec![
        tool_call_with(
            &root,
            1,
            "spawn-refused",
            "agent_spawn",
            r#"{"agent":"ghost","prompt":"nope"}"#,
        ),
        tool_output(
            &root,
            2,
            "spawn-refused",
            "agent_spawn",
            "sub-agent spawn refused: unknown agent profile `ghost`.",
        ),
        tool_call_with(
            &root,
            3,
            "spawn-ok",
            "agent_spawn",
            r#"{"agent":"researcher","prompt":"look into X"}"#,
        ),
        session_started(&child, &root, "researcher"),
        text_delta(&child, 1, "X is..."),
        done(&child, 2),
        tool_output(&root, 4, "spawn-ok", "agent_spawn", &spawn_reply(&child)),
        done(&root, 5),
    ];

    let projected = project(&records, &root);
    // Each agent_spawn call's own ToolOutput flushes independently (no
    // approval pause to batch them): [assistant(refused), tool_result,
    // assistant(ok), tool_result].
    assert_eq!(projected.len(), 4, "{projected:#?}");
    assert!(
        projected[0].content.get("sub_agents").is_none(),
        "the refused call must not get an (incorrect) card: {:#?}",
        projected[0]
    );
    assert_eq!(
        cards(&projected[2]),
        vec![json!({
            "agent_id": child.0,
            "profile": "researcher",
            "task": "look into X",
            "message_count": 1,
            "preview": "X is...",
        })],
        "{projected:#?}"
    );
}

/// Two spawns in one batch: both calls land before either child's records, so
/// nothing about the interleaving is positional — each card must follow its
/// own call's reply text.
#[test]
fn two_spawns_in_one_batch_each_get_their_own_card() {
    let root = SessionId::new("u1:root");
    let first = SessionId::new(CHILD);
    let second = SessionId::new(GRANDCHILD);
    let records = vec![
        tool_call_with(
            &root,
            1,
            "spawn-a",
            "agent_spawn",
            r#"{"agent":"researcher","prompt":"look into A"}"#,
        ),
        tool_call_with(
            &root,
            2,
            "spawn-b",
            "agent_spawn",
            r#"{"agent":"page-writer","prompt":"write up B"}"#,
        ),
        session_started(&second, &root, "page-writer"),
        text_delta(&second, 1, "B written."),
        done(&second, 2),
        session_started(&first, &root, "researcher"),
        text_delta(&first, 1, "A researched."),
        done(&first, 2),
        // Replies come back in the opposite order to the calls — the match is
        // by tool_call_id, so this must not matter.
        tool_output(&root, 3, "spawn-b", "agent_spawn", &spawn_reply(&second)),
        tool_output(&root, 4, "spawn-a", "agent_spawn", &spawn_reply(&first)),
        done(&root, 5),
    ];

    let projected = project(&records, &root);
    let cards = cards(&projected[0]);
    assert_eq!(cards.len(), 2, "{projected:#?}");
    assert_eq!(cards[0]["agent_id"], json!(first.0));
    assert_eq!(cards[0]["task"], json!("look into A"));
    assert_eq!(cards[0]["preview"], json!("A researched."));
    assert_eq!(cards[1]["agent_id"], json!(second.0));
    assert_eq!(cards[1]["task"], json!("write up B"));
    assert_eq!(cards[1]["preview"], json!("B written."));
}
