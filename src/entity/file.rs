use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub hash: String,
    pub mimetype: String,
    #[sea_orm(unique)]
    pub path: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    pub size_bytes: i64,
    pub created_at: DateTimeWithTimeZone,
    pub created_by: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
