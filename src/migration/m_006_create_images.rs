use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_006_create_images"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Images::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Images::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Images::Title).string().not_null())
                    .col(ColumnDef::new(Images::Description).text().null())
                    .col(ColumnDef::new(Images::Data).binary().not_null())
                    .col(ColumnDef::new(Images::Thumbnail).binary().not_null())
                    .col(
                        ColumnDef::new(Images::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Images::CreatedBy).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Images::Table, Images::CreatedBy)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Images::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Images {
    Table,
    Id,
    Title,
    Description,
    Data,
    Thumbnail,
    CreatedAt,
    CreatedBy,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
