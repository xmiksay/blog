use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_032_add_assistant_sessions_parent"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // #99: a spawned `researcher`/`page-writer` sub-agent becomes a real
        // `assistant_sessions` row instead of living only as `LogRecord.session`
        // values inside the root's `assistant_events` file.
        //
        // `parent_session_id` is a self-FK: `ON DELETE CASCADE` so deleting a
        // root takes its whole sub-tree with it (Postgres cascades
        // recursively, so grandchildren go too).
        manager
            .alter_table(
                Table::alter()
                    .table(AssistantSessions::Table)
                    .add_column(
                        ColumnDef::new(AssistantSessions::ParentSessionId)
                            .integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_assistant_sessions_parent_session_id")
                    .from(AssistantSessions::Table, AssistantSessions::ParentSessionId)
                    .to(AssistantSessions::Table, AssistantSessions::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_assistant_sessions_parent_session_id")
                    .table(AssistantSessions::Table)
                    .col(AssistantSessions::ParentSessionId)
                    .to_owned(),
            )
            .await?;

        // The root pointer is the engine id, **not** a row id. `/compact`
        // repoints a root row's `engine_session_id` to a fresh successor
        // session while the pre-compaction log stays filed under the old key
        // (`handlers/sessions/compact.rs`), so an integer self-FK to the root
        // *row* would make a child resolve to the successor's log and read
        // back a blank transcript even though its own data is fully intact.
        // `root_engine_session_id` is exactly the key `load_prior_records`/
        // `resume_session`/`delete_session_events` already filter on, and it
        // never moves out from under a child.
        //
        // Ends up `NOT NULL`: m_031 emptied this table for the 0.6 upgrade, so
        // every row from here on is written with it set (a root row carries
        // its own `engine_session_id`). Added nullable and populated first
        // all the same — a *developer's* database has already been running the
        // post-purge server, so it has rows between m_031 and m_032 that a
        // bare `ADD COLUMN … NOT NULL` would hard-fail on. On a fresh database
        // the `UPDATE` matches nothing and this is the plain add.
        manager
            .alter_table(
                Table::alter()
                    .table(AssistantSessions::Table)
                    .add_column(
                        ColumnDef::new(AssistantSessions::RootEngineSessionId)
                            .text()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE assistant_sessions \
                 SET root_engine_session_id = COALESCE(engine_session_id, '') \
                 WHERE root_engine_session_id IS NULL",
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(AssistantSessions::Table)
                    .modify_column(
                        ColumnDef::new(AssistantSessions::RootEngineSessionId)
                            .text()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_assistant_sessions_root_engine_session_id")
                    .table(AssistantSessions::Table)
                    .col(AssistantSessions::RootEngineSessionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_assistant_sessions_root_engine_session_id")
                    .table(AssistantSessions::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_assistant_sessions_parent_session_id")
                    .table(AssistantSessions::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_assistant_sessions_parent_session_id")
                    .table(AssistantSessions::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(AssistantSessions::Table)
                    .drop_column(AssistantSessions::RootEngineSessionId)
                    .drop_column(AssistantSessions::ParentSessionId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum AssistantSessions {
    Table,
    Id,
    ParentSessionId,
    RootEngineSessionId,
}
