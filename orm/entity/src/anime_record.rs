use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "anime_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub title: String,
    pub anime_id: i64,
    pub magnet: String,
    pub rule_name: String,
    pub info_hash: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
