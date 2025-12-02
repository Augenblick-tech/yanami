use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub id: i64,
    pub timers: u32,
    pub expire: i64,
    pub now: i64,
    pub code: String,
}
