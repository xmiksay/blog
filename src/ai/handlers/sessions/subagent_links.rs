//! Hydrates sub-agent `assistant_sessions` rows straight from a session's
//! persisted log (#99) — the second of the two writers described in
//! `ai::ws_bridge::child_rows` — and splices the resulting row ids onto the
//! projection's sub-agent cards (#100), which is what makes a card clickable.
//!
//! The splice lives here rather than in `ai::projection` on purpose: the
//! projection is a pure fold with no DB access (that is why it can be unit
//! tested without a database at all), and `engine SessionId -> row id` is
//! knowledge only a handler holding a `DatabaseConnection` has.
//!
//! ## Why a handler-side writer at all
//!
//! `ws_bridge`'s live writer has no ordering relationship with the REST
//! handlers: it is an independent `holly.subscribe()`r, so the response for
//! the very turn that spawned a child can be built before that task has
//! written the child's row. The client would then render a card with a null
//! child id — unclickable until some later refetch happened to land after the
//! write. Rebuilding the rows from the log the handler is *already holding*
//! closes that window without any cross-task coordination.
//!
//! It also repairs anything the live writer missed for good: a
//! `RecvError::Lagged` batch swallows the child's `SessionStarted` outright,
//! and that event never comes back on the broadcast.
//!
//! Walking the records in log order gives topological order for free — a
//! grandchild's `SessionStarted` can only appear after its own parent's, so
//! the parent row `ensure_child_row` refuses to guess at is always already
//! written by the time the grandchild is reached.

use std::collections::{HashMap, HashSet};

use entanglement_core::{OutEvent, SessionId};
use entanglement_runtime::session_store::{LogPayload, LogRecord};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};

use crate::ai::projection::ProjectedMessage;
use crate::ai::ws_bridge::child_rows::ensure_child_row;
use crate::entity::assistant_session;

/// Ensure every sub-agent session named by `records` has its own row,
/// returning `engine SessionId -> assistant_sessions.id` for the ones that do.
///
/// Best-effort by design: a child whose parent row is missing is skipped (see
/// [`ensure_child_row`]), never guessed at, and never fails the request it was
/// called from — the transcript itself is unaffected either way.
pub async fn hydrate_child_rows(
    db: &DatabaseConnection,
    records: &[LogRecord],
) -> HashMap<SessionId, i32> {
    let mut ids = HashMap::new();
    for (child, parent, profile) in child_session_starts(records) {
        if let Some(id) = ensure_child_row(db, child, parent, profile).await {
            repair_profile(db, id, profile).await;
            ids.insert(child.clone(), id);
        }
    }
    ids
}

/// Re-point a child row at the profile it was actually spawned under.
///
/// Entanglement 0.6's `SessionStarted` and the `AgentChanged` that follows it
/// always name the same profile: `Session::replay` folds every logged
/// `AgentChanged` before a resumed session re-announces itself, so a normal
/// resume cascade re-announces the *correct* profile, never `build` (verified
/// against 0.6.0 — see `child_session_starts`). The two writers can still
/// disagree in one narrower case: if this child's own persisted log lost its
/// `AgentChanged` record (a genuine gap — a lagged/crashed persistence tap,
/// filed upstream separately), replay has nothing to fold and rebuilds the
/// session under the base `build` profile, so its resume re-announces that
/// instead of the truth. `ws_bridge`'s live writer takes the profile straight
/// off whichever `SessionStarted` broadcast it happens to see and could win
/// the insert with that wrong value, while the log this walks still carries
/// the original (correct) announcement from before the gap. Whichever writer
/// wins the `ON CONFLICT DO NOTHING` insert, this repairs the row before the
/// response that reads it is built.
///
/// A conditional `UPDATE` rather than a read-modify-write `ActiveModel`: it is
/// a no-op write in the overwhelmingly common case (profiles already agree)
/// and can't lose a concurrent hydration's identical write.
async fn repair_profile(db: &DatabaseConnection, id: i32, profile: &str) {
    let res = assistant_session::Entity::update_many()
        .col_expr(
            assistant_session::Column::AgentProfile,
            Expr::value(profile.to_string()),
        )
        .col_expr(
            assistant_session::Column::Title,
            Expr::value(format!("{profile} sub-agent")),
        )
        .filter(assistant_session::Column::Id.eq(id))
        .filter(assistant_session::Column::AgentProfile.ne(profile))
        .exec(db)
        .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, session_id = id, profile, "failed to repair sub-agent profile");
    }
}

/// Splice `child_db_session_id` onto every sub-agent card in `projected`,
/// looked up by the card's own `agent_id` (the child's engine `SessionId`).
///
/// A card whose child has no row — [`hydrate_child_rows`] skipped it, or it
/// belongs to a session tree this user doesn't own — is left without the key
/// rather than given a null: the client's "openable" test is the key's
/// presence, and inventing an id it can't open is worse than a card that
/// stays flat.
pub fn splice_child_db_ids(projected: &mut [ProjectedMessage], ids: &HashMap<SessionId, i32>) {
    for msg in projected.iter_mut() {
        let Some(cards) = msg
            .content
            .get_mut("sub_agents")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for card in cards.iter_mut() {
            let Some(agent_id) = card.get("agent_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(db_id) = ids.get(&SessionId::new(agent_id.to_string())).copied() else {
                continue;
            };
            if let Some(obj) = card.as_object_mut() {
                obj.insert("child_db_session_id".into(), json!(db_id));
            }
        }
    }
}

/// Every `(child, parent, profile)` a spawn announced, in log order, **once
/// per child**. A root's own `SessionStarted` (`parent: None`) is not a child
/// and no other record shape carries a parent link, so this is the whole of
/// the tree structure the log records.
///
/// First announcement wins: a resume re-announces `SessionStarted` for every
/// re-materialized child, and in the ordinary case that re-announcement names
/// the same profile the child was actually spawned under — 0.6's replay folds
/// every logged `AgentChanged` before a resumed session re-announces itself,
/// so `SessionStarted.profile` and the following `AgentChanged.agent` never
/// disagree. The one case where a *later* record can name the wrong agent is
/// a genuine gap in this child's own log: it lost its `AgentChanged` record,
/// so replay has nothing to fold and rebuilds it under the base `build`
/// profile instead (filed upstream separately). Keeping the first
/// announcement recovers the true profile from before that gap — see
/// [`repair_profile`].
fn child_session_starts(records: &[LogRecord]) -> Vec<(&SessionId, &SessionId, &str)> {
    let mut seen = HashSet::new();
    records
        .iter()
        .filter_map(|r| match &r.payload {
            LogPayload::Out(OutEvent::SessionStarted {
                session,
                parent: Some(parent),
                profile,
                ..
            }) if seen.insert(session) => Some((session, parent, profile.as_str())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use entanglement_core::InMsg;

    fn started(session: &SessionId, parent: Option<&SessionId>, profile: &str) -> LogRecord {
        LogRecord::new(
            session.clone(),
            LogPayload::Out(OutEvent::SessionStarted {
                session: session.clone(),
                parent: parent.cloned(),
                predecessor: None,
                profile: profile.into(),
                model: None,
                user: None,
                root: parent.is_none(),
                ts: 0,
            }),
        )
    }

    /// The scan must pick out exactly the spawned children — a root's own
    /// `SessionStarted` (`parent: None`) is not a child, and no other record
    /// shape carries a parent link at all.
    #[test]
    fn selects_only_child_session_starts() {
        let root = SessionId::new("u1:root".to_string());
        let child = SessionId::new_uuid();
        let records = vec![
            started(&root, None, "build"),
            LogRecord::new(
                root.clone(),
                LogPayload::In(InMsg::prompt(root.clone(), "hi")),
            ),
            started(&child, Some(&root), "researcher"),
        ];

        assert_eq!(
            child_session_starts(&records),
            vec![(&child, &root, "researcher")]
        );
    }

    /// Log order is topological order: a grandchild's `SessionStarted` can
    /// only follow its parent's, which is what lets a single forward pass
    /// write the whole tree without a second reconciliation round.
    #[test]
    fn yields_a_parent_before_its_own_grandchild() {
        let root = SessionId::new("u1:root".to_string());
        let child = SessionId::new_uuid();
        let grandchild = SessionId::new_uuid();
        let records = vec![
            started(&root, None, "build"),
            started(&child, Some(&root), "researcher"),
            started(&grandchild, Some(&child), "page-writer"),
        ];

        assert_eq!(
            child_session_starts(&records),
            vec![
                (&child, &root, "researcher"),
                (&grandchild, &child, "page-writer"),
            ]
        );
    }

    /// A gap in a child's own persisted log — it lost its `AgentChanged`
    /// record, a rare upstream persistence hole, not the ordinary resume path
    /// — makes a later re-announcement of that child name the base `build`
    /// profile instead of the truth. The first announcement, from before the
    /// gap, must be what a row is written (or repaired) from.
    #[test]
    fn keeps_only_a_childs_first_announcement() {
        let root = SessionId::new("u1:root".to_string());
        let child = SessionId::new_uuid();
        let records = vec![
            started(&root, None, "build"),
            started(&child, Some(&root), "page-writer"),
            // A resume re-announcement degraded by a lost `AgentChanged`.
            started(&child, Some(&root), "build"),
        ];

        assert_eq!(
            child_session_starts(&records),
            vec![(&child, &root, "page-writer")]
        );
    }

    fn carded(agent_ids: &[&str]) -> ProjectedMessage {
        ProjectedMessage {
            role: "assistant",
            content: json!({
                "text": Value::Null,
                "tool_calls": [],
                "sub_agents": agent_ids
                    .iter()
                    .map(|id| json!({ "agent_id": id, "profile": "researcher" }))
                    .collect::<Vec<_>>(),
            }),
        }
    }

    /// Each card gets the row id of the child *it* names — matching is by
    /// `agent_id`, never by position, since two cards can sit on one message.
    #[test]
    fn splices_each_card_by_its_own_agent_id() {
        let a = SessionId::new_uuid();
        let b = SessionId::new_uuid();
        let mut projected = vec![
            ProjectedMessage {
                role: "user",
                content: json!({ "text": "go" }),
            },
            carded(&[&a.0, &b.0]),
        ];
        let ids = HashMap::from([(a.clone(), 11), (b.clone(), 22)]);

        splice_child_db_ids(&mut projected, &ids);

        let cards = projected[1].content["sub_agents"]
            .as_array()
            .expect("cards");
        assert_eq!(cards[0]["child_db_session_id"], json!(11));
        assert_eq!(cards[1]["child_db_session_id"], json!(22));
        // Untouched: a message with no cards must come through unchanged.
        assert_eq!(projected[0].content, json!({ "text": "go" }));
    }

    /// A child hydration couldn't place gets no key at all — the client tests
    /// for presence, so a null would render an unopenable "openable" card.
    #[test]
    fn leaves_an_unknown_child_without_a_db_id() {
        let unknown = SessionId::new_uuid();
        let mut projected = vec![carded(&[&unknown.0])];

        splice_child_db_ids(&mut projected, &HashMap::new());

        let cards = projected[0].content["sub_agents"]
            .as_array()
            .expect("cards");
        assert!(
            cards[0].get("child_db_session_id").is_none(),
            "{:#}",
            cards[0]
        );
    }
}
