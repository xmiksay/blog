use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set,
};

use crate::entity::token;

pub struct ServiceTokenInput {
    pub user_id: i32,
    pub label: Option<String>,
}

pub struct CreatedToken {
    pub model: token::Model,
    pub nonce: String,
}

pub async fn list_service_tokens(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<Vec<token::Model>, DbErr> {
    token::Entity::find()
        .filter(token::Column::UserId.eq(user_id))
        .filter(token::Column::IsService.eq(true))
        .order_by_desc(token::Column::Id)
        .all(db)
        .await
}

pub async fn create_service_token(
    db: &DatabaseConnection,
    nonce_generator: impl FnOnce() -> String,
    input: ServiceTokenInput,
) -> Result<CreatedToken, DbErr> {
    let nonce = nonce_generator();
    let model = token::ActiveModel {
        nonce: Set(nonce.clone()),
        user_id: Set(input.user_id),
        expires_at: Set(None),
        label: Set(input.label.filter(|s| !s.is_empty())),
        is_service: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(CreatedToken { model, nonce })
}

#[derive(Debug)]
pub enum DeleteError {
    NotFound,
    Db(DbErr),
}

impl From<DbErr> for DeleteError {
    fn from(e: DbErr) -> Self {
        Self::Db(e)
    }
}

/// Delete a service token by id, requiring it belongs to `user_id` and is a
/// service token. Returns `NotFound` for missing rows or rows owned by someone
/// else (so callers can't probe other users' tokens).
pub async fn delete_service_token(
    db: &DatabaseConnection,
    user_id: i32,
    id: i32,
) -> Result<(), DeleteError> {
    let row = token::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DeleteError::NotFound)?;
    if row.user_id != user_id || !row.is_service {
        return Err(DeleteError::NotFound);
    }
    token::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}
