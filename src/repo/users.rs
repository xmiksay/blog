use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set, TransactionTrait,
};

use crate::entity::{file, gallery, page, page_revision, user};

pub struct NewUserInput {
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug)]
pub enum CreateError {
    Conflict,
    Db(DbErr),
}

impl From<DbErr> for CreateError {
    fn from(e: DbErr) -> Self {
        Self::Db(e)
    }
}

#[derive(Debug)]
pub enum UpdateError {
    NotFound,
    Db(DbErr),
}

impl From<DbErr> for UpdateError {
    fn from(e: DbErr) -> Self {
        Self::Db(e)
    }
}

#[derive(Debug)]
pub enum DeleteError {
    SelfDelete,
    NotFound,
    Db(DbErr),
}

impl From<DbErr> for DeleteError {
    fn from(e: DbErr) -> Self {
        Self::Db(e)
    }
}

pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<user::Model>, DbErr> {
    user::Entity::find()
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
}

pub async fn create(
    db: &DatabaseConnection,
    input: NewUserInput,
) -> Result<user::Model, CreateError> {
    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(input.username.as_str()))
        .one(db)
        .await?;
    if existing.is_some() {
        return Err(CreateError::Conflict);
    }
    let model = user::ActiveModel {
        username: Set(input.username),
        password_hash: Set(input.password_hash),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(model)
}

pub async fn update_password(
    db: &DatabaseConnection,
    id: i32,
    password_hash: String,
) -> Result<(), UpdateError> {
    let row = user::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(UpdateError::NotFound)?;
    let mut active: user::ActiveModel = row.into();
    active.password_hash = Set(password_hash);
    active.update(db).await?;
    Ok(())
}

/// Delete `id`, reassigning their authored content (pages, page_revisions,
/// galleries, files) to `deleting_user_id` first. Cascade FKs handle the
/// user-personal rows (tokens, oauth, mcp, tool permissions, assistant).
pub async fn delete(
    db: &DatabaseConnection,
    deleting_user_id: i32,
    id: i32,
) -> Result<(), DeleteError> {
    if id == deleting_user_id {
        return Err(DeleteError::SelfDelete);
    }

    let exists = user::Entity::find_by_id(id).one(db).await?.is_some();
    if !exists {
        return Err(DeleteError::NotFound);
    }

    let txn = db.begin().await?;

    page::Entity::update_many()
        .col_expr(page::Column::CreatedBy, Expr::value(deleting_user_id))
        .filter(page::Column::CreatedBy.eq(id))
        .exec(&txn)
        .await?;

    page::Entity::update_many()
        .col_expr(page::Column::ModifiedBy, Expr::value(deleting_user_id))
        .filter(page::Column::ModifiedBy.eq(id))
        .exec(&txn)
        .await?;

    page_revision::Entity::update_many()
        .col_expr(
            page_revision::Column::CreatedBy,
            Expr::value(deleting_user_id),
        )
        .filter(page_revision::Column::CreatedBy.eq(id))
        .exec(&txn)
        .await?;

    gallery::Entity::update_many()
        .col_expr(gallery::Column::CreatedBy, Expr::value(deleting_user_id))
        .filter(gallery::Column::CreatedBy.eq(id))
        .exec(&txn)
        .await?;

    file::Entity::update_many()
        .col_expr(file::Column::CreatedBy, Expr::value(deleting_user_id))
        .filter(file::Column::CreatedBy.eq(id))
        .exec(&txn)
        .await?;

    user::Entity::delete_by_id(id).exec(&txn).await?;

    txn.commit().await?;
    Ok(())
}
