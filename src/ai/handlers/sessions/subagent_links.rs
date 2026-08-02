//! Hydrates sub-agent `assistant_sessions` rows straight from a session's
//! persisted log (#99) — the second of the two writers described in
//! `ai::ws_bridge::child_rows`.
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

use std::collections::HashMap;

use entanglement_core::{OutEvent, SessionId};
use entanglement_runtime::session_store::{LogPayload, LogRecord};
use sea_orm::DatabaseConnection;

use crate::ai::ws_bridge::child_rows::ensure_child_row;

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
            ids.insert(child.clone(), id);
        }
    }
    ids
}

/// Every `(child, parent, profile)` a spawn announced, in log order. A root's
/// own `SessionStarted` (`parent: None`) is not a child and no other record
/// shape carries a parent link, so this is the whole of the tree structure the
/// log records.
fn child_session_starts(records: &[LogRecord]) -> Vec<(&SessionId, &SessionId, &str)> {
    records
        .iter()
        .filter_map(|r| match &r.payload {
            LogPayload::Out(OutEvent::SessionStarted {
                session,
                parent: Some(parent),
                profile,
                ..
            }) => Some((session, parent, profile.as_str())),
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
}
