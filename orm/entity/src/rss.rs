use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub id: String,
    pub url: Option<String>,
    pub title: String,
    pub search_url: Option<String>,
}
