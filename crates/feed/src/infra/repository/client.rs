use anyhow::Result;
use sqlx::{Pool, Sqlite, Transaction};

#[derive(Clone)]
pub struct FeedSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl FeedSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

impl FeedSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS feed (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                title      TEXT    NOT NULL UNIQUE,
                site_url   TEXT,                      -- 可为空，对应 Option<String>
                search_url TEXT,                      -- 可为空，对应 Option<String>
                source_key TEXT    NOT NULL UNIQUE,   -- 订阅源的唯一标识键
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_feed_source_key ON feed(source_key);")
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}
