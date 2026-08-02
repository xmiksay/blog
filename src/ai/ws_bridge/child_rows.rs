//! Writes a spawned sub-agent's own `assistant_sessions` row (#99).
//!
//! Before #99 a `researcher`/`page-writer` child existed only as
//! `LogRecord.session` values inside `assistant_events` rows filed under the
//! *root's* `root_session_id`, plus two in-memory parent maps
//! (`engine::session_tree`'s `SESSION_PARENTS` and `ws_bridge`'s own
//! `local_parents`). It now gets a real row with a real parent link, so the
//! admin UI can list and open it like any other session.
//!
//! There are three writers racing on the same child: this module called from
//! `ws_bridge`'s live `OutEvent` stream, and
//! `handlers::sessions::subagent_links::hydrate_child_rows` called from
//! whichever REST handler projects the root's log next (`read`,
//! `send_message`/`approve`, `compact`) — the last of which can run
//! concurrently in two requests. Hence `INSERT … ON CONFLICT
//! (engine_session_id) DO NOTHING` (the m_023 unique index is the conflict
//! target) followed by a plain `SELECT`, never check-then-insert.

use entanglement_core::SessionId;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set};

use crate::entity::assistant_session;

/// Ensure `child` (spawned by `parent`, running under `profile`) has an
/// `assistant_sessions` row, returning its id.
///
/// Returns `None` — never a guessed row — when `parent` has no row of its own:
/// that would be a child of a session this site doesn't track, and inventing a
/// `user_id`/`root_engine_session_id` for it would either leak the transcript
/// to the wrong user or file it under a log that doesn't exist. A grandchild
/// spawned before its own parent's row landed hits this path and is picked up
/// later by `hydrate_child_rows`, which walks the log in order.
pub async fn ensure_child_row(
    db: &DatabaseConnection,
    child: &SessionId,
    parent: &SessionId,
    profile: &str,
) -> Option<i32> {
    let parent_row = find_by_engine_id(db, parent).await;
    let Some(active) = child_row_for(parent_row.as_ref(), child, profile) else {
        tracing::warn!(
            child = %child.0,
            parent = %parent.0,
            "refusing to create a sub-agent session row: no parent row"
        );
        return None;
    };

    match assistant_session::Entity::insert(active)
        .on_conflict(
            OnConflict::column(assistant_session::Column::EngineSessionId)
                .do_nothing()
                .to_owned(),
        )
        .exec(db)
        .await
    {
        Ok(res) => Some(res.last_insert_id),
        // A concurrent writer got there first — expected, not an error.
        Err(DbErr::RecordNotInserted) => find_by_engine_id(db, child).await.map(|row| row.id),
        Err(e) => {
            tracing::error!(error = %e, child = %child.0, "failed to create sub-agent session row");
            None
        }
    }
}

async fn find_by_engine_id(
    db: &DatabaseConnection,
    session: &SessionId,
) -> Option<assistant_session::Model> {
    assistant_session::Entity::find()
        .filter(assistant_session::Column::EngineSessionId.eq(session.0.clone()))
        .one(db)
        .await
        .inspect_err(
            |e| tracing::error!(error = %e, session = %session.0, "failed to look up session row"),
        )
        .ok()?
}

/// Derive the child's row from its spawning parent's. Pure, so the derivation
/// rules are unit-testable without a database.
///
/// `user_id` comes from the parent **row**, never `engine::user_id_from_session`
/// — that walks `SESSION_PARENTS`, which `session_tree::evict_on_hibernate_or_end`
/// drops on hibernate/end, so it fails for every settled child and would leave
/// hydration unable to place a row at all.
///
/// `root_engine_session_id` is likewise inherited from the parent row rather
/// than assumed to be the parent's own `engine_session_id`, which is what makes
/// a grandchild file under the same root as its parent.
///
/// The generation overrides (`temperature`, `reasoning_effort`, …) are
/// deliberately *not* inherited: nothing sends the child an
/// `InMsg::SetGeneration`, so it genuinely runs on the model's defaults and
/// copying the parent's knobs would display an override that isn't in effect.
fn child_row_for(
    parent: Option<&assistant_session::Model>,
    child: &SessionId,
    profile: &str,
) -> Option<assistant_session::ActiveModel> {
    let parent = parent?;
    let now = chrono::Utc::now().fixed_offset();
    Some(assistant_session::ActiveModel {
        user_id: Set(parent.user_id),
        title: Set(format!("{profile} sub-agent")),
        provider: Set(parent.provider.clone()),
        model: Set(parent.model.clone()),
        model_id: Set(parent.model_id),
        enabled_mcp_server_ids: Set(serde_json::json!([])),
        engine_session_id: Set(Some(child.0.clone())),
        parent_session_id: Set(Some(parent.id)),
        root_engine_session_id: Set(parent.root_engine_session_id.clone()),
        agent_profile: Set(profile.to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::engine::SiteEngine;
    use sea_orm::ActiveValue;

    fn parent_row(id: i32) -> assistant_session::Model {
        let now = chrono::Utc::now().fixed_offset();
        assistant_session::Model {
            id,
            user_id: 77,
            title: "New chat".into(),
            provider: "ollama".into(),
            model: "qwen3".into(),
            model_id: Some(5),
            enabled_mcp_server_ids: serde_json::json!([1, 2]),
            engine_session_id: Some(SiteEngine::session_id_for_user(77).0),
            parent_session_id: None,
            root_engine_session_id: "u77:root-log".into(),
            temperature: Some(0.4),
            reasoning_effort: Some("high".into()),
            max_output_tokens: Some(1234),
            thinking_budget_tokens: Some(999),
            agent_profile: "build".into(),
            created_at: now,
            updated_at: now,
        }
    }

    fn taken<T>(v: &ActiveValue<T>) -> &T
    where
        sea_orm::Value: From<T>,
    {
        match v {
            ActiveValue::Set(v) => v,
            _ => panic!("expected a Set value"),
        }
    }

    #[test]
    fn derives_the_child_row_from_its_parent_row() {
        let parent = parent_row(42);
        let child = SessionId::new_uuid();
        let am = child_row_for(Some(&parent), &child, "researcher").expect("derives a row");

        assert_eq!(*taken(&am.parent_session_id), Some(42));
        // Copied from the parent *row*, never `user_id_from_session` — see the
        // function's doc.
        assert_eq!(*taken(&am.user_id), 77);
        // Inherited, not re-derived from the parent's own engine id: that is
        // what files a grandchild under the same root.
        assert_eq!(*taken(&am.root_engine_session_id), "u77:root-log");
        assert_eq!(*taken(&am.engine_session_id), Some(child.0.clone()));
        assert_eq!(*taken(&am.agent_profile), "researcher");
        assert_eq!(*taken(&am.provider), "ollama");
        assert_eq!(*taken(&am.model), "qwen3");
        assert_eq!(*taken(&am.model_id), Some(5));
        // A child gets no MCP servers of its own and no generation overrides —
        // nothing ever mirrors either onto its engine session.
        assert_eq!(*taken(&am.enabled_mcp_server_ids), serde_json::json!([]));
        assert!(matches!(am.temperature, ActiveValue::NotSet));
        assert!(matches!(am.reasoning_effort, ActiveValue::NotSet));
    }

    /// A grandchild inherits its *parent child's* root pointer, so the whole
    /// sub-tree stays filed under one `assistant_events` key.
    #[test]
    fn grandchild_inherits_the_root_engine_session_id() {
        let mut middle = parent_row(42);
        middle.id = 43;
        middle.parent_session_id = Some(42);
        middle.engine_session_id = Some(SessionId::new_uuid().0);

        let grandchild = SessionId::new_uuid();
        let am = child_row_for(Some(&middle), &grandchild, "page-writer").expect("derives a row");

        assert_eq!(*taken(&am.parent_session_id), Some(43));
        assert_eq!(*taken(&am.root_engine_session_id), "u77:root-log");
    }

    /// Never default, never guess: without a real parent row there is no
    /// trustworthy `user_id` or root log key to file the child under.
    #[test]
    fn refuses_to_derive_a_row_without_a_real_parent_row() {
        let child = SessionId::new_uuid();
        assert!(child_row_for(None, &child, "researcher").is_none());
    }
}
