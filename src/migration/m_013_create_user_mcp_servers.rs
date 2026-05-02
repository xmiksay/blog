use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_013_create_user_mcp_servers"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserMcpServers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserMcpServers::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserMcpServers::UserId).integer().not_null())
                    .col(ColumnDef::new(UserMcpServers::Name).string().not_null())
                    .col(ColumnDef::new(UserMcpServers::Url).string().not_null())
                    .col(
                        ColumnDef::new(UserMcpServers::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(UserMcpServers::ForwardUserToken)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(UserMcpServers::Headers)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'{}'::jsonb")),
                    )
                    .col(
                        ColumnDef::new(UserMcpServers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserMcpServers::Table, UserMcpServers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_mcp_servers_user_name")
                    .table(UserMcpServers::Table)
                    .col(UserMcpServers::UserId)
                    .col(UserMcpServers::Name)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserMcpServers::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum UserMcpServers {
    Table,
    Id,
    UserId,
    Name,
    Url,
    Enabled,
    ForwardUserToken,
    Headers,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
