pub mod admin;
pub mod mcp;
pub mod public;
pub mod revision;

use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};

use crate::entity::menu;

#[derive(serde::Serialize)]
pub struct MenuItem {
    pub path: String,
    pub label: String,
}

pub async fn build_menu(db: &DatabaseConnection) -> Vec<MenuItem> {
    menu::Entity::find()
        .order_by_asc(menu::Column::OrderIndex)
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| MenuItem {
            label: m.title,
            path: format!("/{}", m.path),
        })
        .collect()
}
