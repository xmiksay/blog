use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "file_thumbnails")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: i32,
    pub hash: String,
    pub width: i32,
    pub height: i32,
    pub mimetype: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
