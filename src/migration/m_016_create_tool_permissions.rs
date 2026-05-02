use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_016_create_tool_permissions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ToolPermissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ToolPermissions::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ToolPermissions::UserId).integer().not_null())
                    .col(ColumnDef::new(ToolPermissions::Name).string().not_null())
                    .col(ColumnDef::new(ToolPermissions::Effect).string().not_null())
                    .col(
                        ColumnDef::new(ToolPermissions::Priority)
                            .integer()
                            .not_null()
                            .default(100),
                    )
                    .col(
                        ColumnDef::new(ToolPermissions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ToolPermissions::Table, ToolPermissions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_tool_permissions_user")
                    .table(ToolPermissions::Table)
                    .col(ToolPermissions::UserId)
                    .col(ToolPermissions::Priority)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ToolPermissions::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum ToolPermissions {
    Table,
    Id,
    UserId,
    Name,
    Effect,
    Priority,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
