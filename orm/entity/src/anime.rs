use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "anime")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub status: bool,
    pub rule_name: String,
    pub anime_info: serde_json::Value,
    pub is_search: bool,
    pub is_lock: bool,
    pub progress: u8,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
