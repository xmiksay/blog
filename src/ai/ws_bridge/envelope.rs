//! Builds the client-facing session identity spliced onto every forwarded
//! `OutEvent` payload (`db_session_id` / `agent_session_id` /
//! `child_db_session_id`).
//!
//! Split out of `mod.rs` at the "what the client sees" seam: the forwarding
//! loop owns *which* events go out and to whom, this owns *how* the engine's
//! session identifiers are named on the wire — and it is the part worth
//! testing without a hub or a database.

use dashmap::DashMap;
use entanglement_core::SessionId;
use serde_json::{Value, json};

/// A sub-agent child's own `assistant_sessions.id` (#102), or `None` for a
/// root event — a root's row id is already `db_session_id`.
///
/// Deliberately cache-only. The entry is seeded by `forward` from
/// `child_rows::ensure_child_row` the moment the child's `SessionStarted`
/// comes through, so the normal path always hits. A child whose row never
/// landed (its `SessionStarted` lost to `RecvError::Lagged`, or its parent row
/// not yet written) would otherwise re-query the DB for every one of its
/// dozens of deltas; instead the field is simply omitted and the client keeps
/// rendering the child under the root, exactly as it did before #102.
pub fn child_db_session_id(
    cache: &DashMap<String, i32>,
    session: &SessionId,
    root: &SessionId,
) -> Option<i32> {
    if session == root {
        return None;
    }
    cache.get(&session.0).map(|id| *id)
}

/// Splice the session identity onto a serialized `OutEvent`.
///
/// `db_session_id` names the **root's** row for every event, a child's
/// included — the client keys its inline running-sub-agent card on it and
/// refetches the parent when a child settles, so repurposing it to mean the
/// child would make that card vanish and the parent never refresh (#102).
/// The child's own identifiers are added beside it, never in place of it:
/// `agent_session_id` (its engine `SessionId`) and, when its row is known,
/// `child_db_session_id`. A root event carries neither, keeping the envelope
/// byte-identical to pre-#17 for any session that never spawns a sub-agent.
pub fn splice_session_ids(
    payload: &mut Value,
    db_session_id: i32,
    session: &SessionId,
    root: &SessionId,
    child_db_session_id: Option<i32>,
) {
    let Some(obj) = payload.as_object_mut() else {
        return;
    };
    obj.insert("db_session_id".into(), json!(db_session_id));
    if session == root {
        return;
    }
    obj.insert("agent_session_id".into(), json!(session.0));
    if let Some(child_db_session_id) = child_db_session_id {
        obj.insert("child_db_session_id".into(), json!(child_db_session_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::engine::SiteEngine;

    fn payload() -> Value {
        json!({ "kind": "text_delta", "text": "hi" })
    }

    #[test]
    fn a_root_event_carries_only_the_db_session_id() {
        let root = SiteEngine::session_id_for_user(1);
        let mut p = payload();
        splice_session_ids(&mut p, 7, &root, &root, None);

        assert_eq!(p["db_session_id"], json!(7));
        assert!(p.get("agent_session_id").is_none());
        assert!(p.get("child_db_session_id").is_none());
    }

    /// The whole point of #102: the child's row id is *additive*, and
    /// `db_session_id` still names the root so the parent view keeps rendering
    /// the inline sub-agent card and still refetches when the child settles.
    #[test]
    fn a_child_event_carries_the_root_id_plus_both_child_identifiers() {
        let root = SiteEngine::session_id_for_user(1);
        let child = SessionId::new_uuid();
        let mut p = payload();
        splice_session_ids(&mut p, 7, &child, &root, Some(9));

        assert_eq!(p["db_session_id"], json!(7));
        assert_eq!(p["agent_session_id"], json!(child.0));
        assert_eq!(p["child_db_session_id"], json!(9));
    }

    /// Degrade, don't drop: an unknown child row omits the field rather than
    /// suppressing the event — the client falls back to the root's stream.
    #[test]
    fn a_child_with_no_known_row_still_streams_under_the_root() {
        let root = SiteEngine::session_id_for_user(1);
        let child = SessionId::new_uuid();
        let mut p = payload();
        splice_session_ids(&mut p, 7, &child, &root, None);

        assert_eq!(p["db_session_id"], json!(7));
        assert_eq!(p["agent_session_id"], json!(child.0));
        assert!(p.get("child_db_session_id").is_none());
    }

    #[test]
    fn child_db_session_id_reads_the_seeded_cache_and_never_a_root() {
        let root = SiteEngine::session_id_for_user(1);
        let child = SessionId::new_uuid();
        let cache: DashMap<String, i32> = DashMap::new();
        cache.insert(root.0.clone(), 7);
        cache.insert(child.0.clone(), 9);

        assert_eq!(child_db_session_id(&cache, &child, &root), Some(9));
        assert_eq!(child_db_session_id(&cache, &root, &root), None);
        assert_eq!(
            child_db_session_id(&cache, &SessionId::new_uuid(), &root),
            None,
            "a child whose row never landed omits the field instead of guessing one"
        );
    }
}
