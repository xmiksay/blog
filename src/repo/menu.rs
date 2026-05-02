use sea_orm::{ActiveModelTrait, DatabaseConnection, DbErr, EntityTrait, QueryOrder, Set};

use crate::entity::menu;
use crate::path_util;

pub struct MenuInput {
    pub title: String,
    pub path: String,
    pub markdown: String,
    pub order_index: i32,
    pub private: bool,
}

pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<menu::Model>, DbErr> {
    menu::Entity::find()
        .order_by_asc(menu::Column::OrderIndex)
        .all(db)
        .await
}

pub async fn find_by_id(db: &DatabaseConnection, id: i32) -> Result<Option<menu::Model>, DbErr> {
    menu::Entity::find_by_id(id).one(db).await
}

pub async fn create(db: &DatabaseConnection, input: MenuInput) -> Result<menu::Model, DbErr> {
    menu::ActiveModel {
        title: Set(input.title),
        path: Set(path_util::normalize(&input.path)),
        markdown: Set(input.markdown),
        order_index: Set(input.order_index),
        private: Set(input.private),
        ..Default::default()
    }
    .insert(db)
    .await
}

pub async fn update(
    db: &DatabaseConnection,
    id: i32,
    input: MenuInput,
) -> Result<Option<menu::Model>, DbErr> {
    let Some(model) = menu::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };
    let mut active: menu::ActiveModel = model.into();
    active.title = Set(input.title);
    active.path = Set(path_util::normalize(&input.path));
    active.markdown = Set(input.markdown);
    active.order_index = Set(input.order_index);
    active.private = Set(input.private);
    Ok(Some(active.update(db).await?))
}

pub async fn delete_by_id(db: &DatabaseConnection, id: i32) -> Result<bool, DbErr> {
    let res = menu::Entity::delete_by_id(id).exec(db).await?;
    Ok(res.rows_affected > 0)
}
