use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_031_purge_assistant_history"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // `assistant_events.payload` stores a serialized
        // `entanglement_runtime::session_store::LogRecord` — a third-party
        // enum whose wire shape moved across the 0.4 → 0.6 upgrade (`ask_user`
        // v2's `OutEvent::UserQuestion`, `InMsg::Spawn.user`,
        // `OutEvent::Plan.path`, …). Both readers hard-error on a row they
        // can't deserialize (`ai::persistence::resume_session` and
        // `ai::handlers::sessions::turn::load_prior_records`), so one
        // unreadable row takes its whole session down rather than degrading.
        //
        // Settled decision: assistant history is disposable, so the upgrade
        // purges it instead of carrying a per-variant compatibility shim
        // forward — the same call `m_023` made when the engine was adopted.
        // `assistant_events` has no FK to `assistant_sessions` (deliberate,
        // see m_023), so it needs its own delete; `tool_permissions` and the
        // provider/model/MCP-server config rows are user configuration, not
        // history, and are deliberately left alone.
        let db = manager.get_connection();
        db.execute_unprepared("DELETE FROM assistant_events")
            .await?;
        db.execute_unprepared("DELETE FROM assistant_sessions")
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Lossy, irreversible: `up()` deleted rows and nothing captured them.
        // Same precedent as `m_023_create_assistant_events`.
        Err(DbErr::Migration(
            "m_031_purge_assistant_history is not reversible (data deleted)".into(),
        ))
    }
}
