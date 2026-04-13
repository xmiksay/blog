pub mod admin;
pub mod mcp;
pub mod public;
pub mod revision;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use crate::entity::menu;

#[derive(serde::Serialize)]
pub struct MenuItem {
    pub path: String,
    pub label: String,
}

pub async fn build_menu(db: &DatabaseConnection, logged_in: bool) -> Vec<MenuItem> {
    let mut query = menu::Entity::find().order_by_asc(menu::Column::OrderIndex);
    if !logged_in {
        query = query.filter(menu::Column::Private.eq(false));
    }
    query
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
