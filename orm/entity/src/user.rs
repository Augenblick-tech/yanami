use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub chatacter: String,
}
