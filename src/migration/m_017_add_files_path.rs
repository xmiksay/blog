use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_017_add_files_path"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .add_column(ColumnDef::new(Files::Path).string().null())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        db.execute_unprepared("UPDATE files SET path = title WHERE path IS NULL")
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .modify_column(ColumnDef::new(Files::Path).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_files_path")
                    .table(Files::Table)
                    .col(Files::Path)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .drop_column(Files::Title)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .add_column(ColumnDef::new(Files::Title).string().null())
                    .to_owned(),
            )
            .await?;
        let db = manager.get_connection();
        db.execute_unprepared(
            "UPDATE files SET title = regexp_replace(path, '^.*/', '') WHERE title IS NULL",
        )
        .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .modify_column(ColumnDef::new(Files::Title).string().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_files_path")
                    .table(Files::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .drop_column(Files::Path)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Files {
    Table,
    Path,
    Title,
}
