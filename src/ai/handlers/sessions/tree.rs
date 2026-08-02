//! Root-vs-child resolution for the `{id}`-keyed session handlers (#101).
//!
//! Since #99 an `assistant_sessions` row can be a sub-agent **child**: it has
//! its own `engine_session_id`, but its `assistant_events` are filed under its
//! tree's root (`root_engine_session_id`). Two consequences every handler that
//! takes an `{id}` has to respect:
//!
//! - **Never `ensure_live` a child's own engine id.** That resolves to zero
//!   `assistant_events` rows, so `persistence::resume_session` would resume it
//!   from an empty log — materializing a *blank* in-memory session under the
//!   child's id and caching it as live, which permanently desyncs it from the
//!   history still sitting intact in the root's log. [`root_engine_id`] is the
//!   only correct log key; it equals the row's own id on a root, so using it
//!   unconditionally is a no-op there.
//! - **Some operations only make sense on a root.** A child can't take a
//!   model/profile switch of its own, can't be compacted (the successor fork
//!   would repoint it at a `u{id}:`-shaped root id), and can't be deleted on
//!   its own without orphaning events filed under the root while removing the
//!   only row that can read them. [`require_root`] refuses all three with a
//!   `409`.
//!
//! [`order_for_tree`] is the read-side counterpart: `list` stays a flat array,
//! but ordered so the client can rebuild the tree without a second query.

use entanglement_core::SessionId;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::assistant_session;
use crate::routes::api::error::{ApiError, ApiResult};

/// The engine session `row`'s `assistant_events` are filed under — the row's
/// own id on a root, its tree's root on a sub-agent child.
///
/// A plain column read, not an ancestor walk: `root_engine_session_id` is
/// `NOT NULL` (m_032) and backfilled for every pre-existing row, so the value
/// is always the whole answer.
pub(super) fn root_engine_id(row: &assistant_session::Model) -> SessionId {
    SessionId::new(row.root_engine_session_id.clone())
}

/// Refuse `op` on a sub-agent child row — see the module doc for why each of
/// `update`/`compact`/`delete` is root-only. `409` rather than `404`: the row
/// exists and the caller may legitimately own it, the *operation* is what
/// doesn't apply.
pub(super) fn require_root(row: &assistant_session::Model, op: &str) -> ApiResult<()> {
    if row.parent_session_id.is_none() {
        return Ok(());
    }
    Err(ApiError::Conflict(format!(
        "cannot {op} a sub-agent session; act on its parent session instead"
    )))
}

/// Every descendant row of `id`, breadth-first (children, then grandchildren…).
///
/// The self-FK cascades on delete in Postgres, so this is not what removes the
/// rows — it is what lets the handler retire each descendant's *engine* session
/// first, since neither `live_sessions` nor `SESSION_PARENTS` is reachable from
/// a DB cascade.
pub(super) async fn descendants(
    db: &DatabaseConnection,
    id: i32,
) -> Result<Vec<assistant_session::Model>, sea_orm::DbErr> {
    let mut found = Vec::new();
    let mut frontier = vec![id];
    // Bounded by the tree's depth, and `entanglement_runtime` caps a root's
    // spawn budget at 16, so this is a handful of queries at worst.
    while !frontier.is_empty() {
        let rows = assistant_session::Entity::find()
            .filter(assistant_session::Column::ParentSessionId.is_in(frontier))
            .all(db)
            .await?;
        frontier = rows.iter().map(|r| r.id).collect();
        found.extend(rows);
    }
    Ok(found)
}

/// Flatten `rows` into a stable tree order: roots newest-first by `updated_at`,
/// each immediately followed by its own descendants in spawn order
/// (`created_at` ascending within a parent).
///
/// `list` deliberately stays a flat array — the client builds the tree from
/// `parent_session_id` — but a naive flat `updated_at DESC` interleaves the
/// levels: a child row is created *during* a turn, so it is touched after its
/// own parent and sorts above it. Pre-order emission means the client can
/// render the array top to bottom and never sees a child before its parent.
pub(super) fn order_for_tree(rows: Vec<assistant_session::Model>) -> Vec<assistant_session::Model> {
    let nodes: Vec<TreeNode> = rows
        .iter()
        .map(|r| TreeNode {
            id: r.id,
            parent_id: r.parent_session_id,
            created_at: r.created_at.timestamp_micros(),
            updated_at: r.updated_at.timestamp_micros(),
        })
        .collect();
    let mut rows: Vec<Option<assistant_session::Model>> = rows.into_iter().map(Some).collect();
    tree_order(&nodes)
        .into_iter()
        .filter_map(|i| rows.get_mut(i).and_then(Option::take))
        .collect()
}

/// The ordering key of one row, split out from `assistant_session::Model` so
/// [`tree_order`] is unit-testable without building 17-field rows.
struct TreeNode {
    id: i32,
    parent_id: Option<i32>,
    created_at: i64,
    updated_at: i64,
}

/// Indices into `nodes`, in the order [`order_for_tree`] documents. Every index
/// appears exactly once: a node whose parent isn't in `nodes` (it belongs to
/// another user, or the query raced a delete) is emitted as its own root, and a
/// node the walk somehow never reached is appended at the end — `list` must
/// never silently lose a row the user owns.
fn tree_order(nodes: &[TreeNode]) -> Vec<usize> {
    let present: std::collections::HashSet<i32> = nodes.iter().map(|n| n.id).collect();
    let mut roots: Vec<usize> = (0..nodes.len())
        .filter(|&i| !nodes[i].parent_id.is_some_and(|p| present.contains(&p)))
        .collect();
    // Newest-touched root first; id as a deterministic tie-break for rows
    // written within the same clock tick (a test fixture, or a fast fork).
    roots.sort_by_key(|&i| {
        (
            std::cmp::Reverse(nodes[i].updated_at),
            std::cmp::Reverse(nodes[i].id),
        )
    });

    let mut order = Vec::with_capacity(nodes.len());
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        let mut kids: Vec<usize> = (0..nodes.len())
            .filter(|&k| nodes[k].parent_id == Some(nodes[i].id) && k != i)
            .collect();
        kids.sort_by_key(|&k| (nodes[k].created_at, nodes[k].id));
        // Pushed reversed so the first child is popped (and emitted) first.
        stack.extend(kids.into_iter().rev());
    }
    // Totality backstop: `parent_session_id` is set once, to an already-existing
    // row, so a parent cycle can't be built through the API — but if one ever
    // existed its members would have no root to descend from and would vanish
    // from the sidebar. Append instead.
    let emitted: std::collections::HashSet<usize> = order.iter().copied().collect();
    order.extend((0..nodes.len()).filter(|i| !emitted.contains(i)));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, parent_id: Option<i32>, created_at: i64, updated_at: i64) -> TreeNode {
        TreeNode {
            id,
            parent_id,
            created_at,
            updated_at,
        }
    }

    fn ids(nodes: &[TreeNode]) -> Vec<i32> {
        tree_order(nodes).into_iter().map(|i| nodes[i].id).collect()
    }

    /// The bug the ordering exists to fix: the child is touched *after* its own
    /// parent (it is created mid-turn), so a flat `updated_at DESC` puts it
    /// first. Pre-order must put the parent back in front of it.
    #[test]
    fn a_child_never_sorts_above_its_own_parent() {
        let nodes = vec![node(1, None, 10, 10), node(2, Some(1), 20, 20)];
        assert_eq!(ids(&nodes), vec![1, 2]);
    }

    /// Roots stay newest-first, and each one carries its own subtree with it
    /// instead of the levels interleaving.
    #[test]
    fn roots_are_newest_first_each_followed_by_its_own_children() {
        let nodes = vec![
            node(1, None, 10, 10),
            node(2, Some(1), 20, 20),
            node(3, None, 30, 90),
            node(4, Some(3), 40, 40),
        ];
        assert_eq!(ids(&nodes), vec![3, 4, 1, 2]);
    }

    /// Siblings run in spawn order — the order the transcript's sub-agent cards
    /// appear in, not reverse-chronological like the roots.
    #[test]
    fn siblings_are_ordered_by_creation_ascending() {
        let nodes = vec![
            node(1, None, 10, 10),
            node(3, Some(1), 50, 50),
            node(2, Some(1), 20, 20),
        ];
        assert_eq!(ids(&nodes), vec![1, 2, 3]);
    }

    /// A grandchild follows its own parent, not the root it shares a log with.
    #[test]
    fn a_grandchild_follows_its_own_parent() {
        let nodes = vec![
            node(1, None, 10, 10),
            node(2, Some(1), 20, 20),
            node(3, Some(2), 30, 30),
            node(4, Some(1), 40, 40),
        ];
        assert_eq!(ids(&nodes), vec![1, 2, 3, 4]);
    }

    /// A row whose parent isn't in the result set must still be emitted —
    /// dropping it would make a session disappear from the sidebar entirely.
    #[test]
    fn an_orphan_is_emitted_as_its_own_root() {
        let nodes = vec![node(1, None, 10, 10), node(9, Some(404), 20, 20)];
        let ordered = ids(&nodes);
        assert_eq!(ordered.len(), 2, "{ordered:?}");
        assert!(ordered.contains(&9), "{ordered:?}");
    }
}
