use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "rss")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub url: Option<String>,
    pub title: String,
    pub search_url: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
