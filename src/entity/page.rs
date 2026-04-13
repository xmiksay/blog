use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "pages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub path: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub summary: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub markdown: String,
    pub tag_ids: Vec<i32>,
    pub private: bool,
    pub created_at: DateTimeWithTimeZone,
    pub created_by: i32,
    pub modified_at: DateTimeWithTimeZone,
    pub modified_by: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::page_revision::Entity")]
    PageRevisions,
}

impl Related<super::page_revision::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PageRevisions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
