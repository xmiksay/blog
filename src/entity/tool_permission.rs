use sea_orm::entity::prelude::*;

/// Permission rule for an MCP / local tool. Matches by tool `name` (exact or
/// trailing `*` glob). `effect` is `"allow"`, `"deny"`, or `"prompt"`.
/// First-match wins, ordered by `priority` ASC (lower runs first), then `id`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize)]
#[sea_orm(table_name = "tool_permissions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    /// Tool name to match. `*` at the end is treated as a prefix wildcard.
    pub name: String,
    pub effect: String,
    pub priority: i32,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
