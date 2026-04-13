pub use sea_orm_migration::prelude::*;

mod m_001_create_users;
mod m_002_create_tokens;
mod m_003_create_tags;
mod m_004_create_menus;
mod m_005_create_pages;
mod m_006_create_images;
mod m_007_create_galleries;
mod m_008_add_menu_private;
mod m_009_create_oauth;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m_001_create_users::Migration),
            Box::new(m_002_create_tokens::Migration),
            Box::new(m_003_create_tags::Migration),
            Box::new(m_004_create_menus::Migration),
            Box::new(m_005_create_pages::Migration),
            Box::new(m_006_create_images::Migration),
            Box::new(m_007_create_galleries::Migration),
            Box::new(m_008_add_menu_private::Migration),
            Box::new(m_009_create_oauth::Migration),
        ]
    }
}
