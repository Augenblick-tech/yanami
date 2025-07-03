use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub name: String,
    pub cost: u32,
    pub re: String,
}
