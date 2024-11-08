use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "register")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub timers: u32,
    pub expire: i64,
    pub now: i64,
    pub code: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
