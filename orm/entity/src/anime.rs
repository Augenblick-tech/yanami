use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    // #[sea_orm(primary_key)]
    pub id: i64,
    pub status: bool,
    pub rule_name: String,
    pub anime_info: serde_json::Value,
    pub is_search: bool,
    pub is_lock: bool,
    pub progress: u32,
}
