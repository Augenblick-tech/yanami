use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub name: String,
    pub cost: u32,
    pub re: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
