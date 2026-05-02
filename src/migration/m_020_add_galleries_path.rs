use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_020_add_galleries_path"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Galleries::Table)
                    .add_column(ColumnDef::new(Galleries::Path).string().null())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            UPDATE galleries g
            SET path = sub.slug || CASE WHEN sub.rn = 1 THEN '' ELSE '-' || sub.rn::text END
            FROM (
                SELECT
                    id,
                    NULLIF(
                        regexp_replace(
                            regexp_replace(lower(title), '[^a-z0-9]+', '-', 'g'),
                            '(^-+|-+$)', '', 'g'
                        ),
                        ''
                    ) AS slug_raw,
                    COALESCE(
                        NULLIF(
                            regexp_replace(
                                regexp_replace(lower(title), '[^a-z0-9]+', '-', 'g'),
                                '(^-+|-+$)', '', 'g'
                            ),
                            ''
                        ),
                        'gallery-' || id::text
                    ) AS slug,
                    ROW_NUMBER() OVER (
                        PARTITION BY COALESCE(
                            NULLIF(
                                regexp_replace(
                                    regexp_replace(lower(title), '[^a-z0-9]+', '-', 'g'),
                                    '(^-+|-+$)', '', 'g'
                                ),
                                ''
                            ),
                            'gallery-' || id::text
                        )
                        ORDER BY id
                    ) AS rn
                FROM galleries
            ) sub
            WHERE g.id = sub.id AND g.path IS NULL
            "#,
        )
        .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Galleries::Table)
                    .modify_column(ColumnDef::new(Galleries::Path).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_galleries_path")
                    .table(Galleries::Table)
                    .col(Galleries::Path)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_galleries_path")
                    .table(Galleries::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Galleries::Table)
                    .drop_column(Galleries::Path)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Galleries {
    Table,
    Path,
}
