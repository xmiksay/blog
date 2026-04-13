use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_008_add_menu_private"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Menus::Table)
                    .add_column(
                        ColumnDef::new(Menus::Private)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Menus::Table)
                    .drop_column(Menus::Private)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Menus {
    Table,
    Private,
}
