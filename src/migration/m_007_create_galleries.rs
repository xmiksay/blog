use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_007_create_galleries"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Galleries::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Galleries::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Galleries::Title).string().not_null())
                    .col(ColumnDef::new(Galleries::Description).text().null())
                    .col(
                        ColumnDef::new(Galleries::ImageIds)
                            .array(ColumnType::Integer)
                            .not_null()
                            .default(Expr::cust("'{}'::int[]")),
                    )
                    .col(
                        ColumnDef::new(Galleries::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Galleries::CreatedBy).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Galleries::Table, Galleries::CreatedBy)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Galleries::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Galleries {
    Table,
    Id,
    Title,
    Description,
    ImageIds,
    CreatedAt,
    CreatedBy,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
