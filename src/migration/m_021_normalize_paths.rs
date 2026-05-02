use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m_021_normalize_paths"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // strip leading/trailing slashes, collapse repeated slashes, lowercase
        let normalize = "lower(regexp_replace(btrim(path, '/'), '/+', '/', 'g'))";
        for table in ["pages", "galleries", "files", "menus"] {
            let sql = format!("UPDATE {table} SET path = {normalize} WHERE path <> {normalize}");
            db.execute_unprepared(&sql).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
