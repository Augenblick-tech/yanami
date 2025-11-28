use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub id: String,
    pub url: Option<String>,
    pub title: String,
    pub search_url: Option<String>,
}


#[derive(Debug, Clone, FromRow)]
pub struct RssRecordModel {
    pub title: String,
    pub magnet: String,
    pub info_hash: String,
    pub created_time: Option<i64>,
    pub source: Option<String>,
    pub info: Option<String>,
    pub url: Option<String>,
}

