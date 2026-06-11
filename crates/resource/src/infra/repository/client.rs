use anyhow::{Result, anyhow};
use sqlx::{Pool, QueryBuilder, Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::entity::model::{ResourceBaseData, ResourceProp};

#[derive(Clone)]
pub struct ResourceSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl ResourceSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

impl ResourceSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS resource (
                info_hash       BLOB    NOT NULL PRIMARY KEY,
                title           TEXT    NOT NULL,
                match_title     TEXT    NOT NULL,
                url             TEXT    NOT NULL,
                published_at    INTEGER NOT NULL,
                created_at      INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_resource_published_at ON resource(published_at);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS resource_url_info_hash (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                url         TEXT    NOT NULL UNIQUE,
                info_hash   BLOB    NOT NULL
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_feed_item_url ON resource_url_info_hash(url);")
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}

impl ResourceSqliteClient {
    pub(super) fn parse_resource_row(row: &SqliteRow) -> Result<ResourceBaseData> {
        let info_hash_blob: Vec<u8> = row.try_get("info_hash")?;
        let info_hash: [u8; 20] = info_hash_blob
            .try_into()
            .map_err(|_| anyhow!("info_hash length is not 20"))?;

        Ok(ResourceBaseData {
            info_hash,
            title: row.try_get("title")?,
            match_title: row.try_get("match_title")?,
            url: row.try_get("url")?,
            published_at: row.try_get("published_at")?,
        })
    }

    pub(super) async fn batch_insert_url_hash(
        &self,
        tx: &mut sqlx::SqliteConnection,
        chunk: &[ResourceBaseData],
    ) -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }

        // 使用 INSERT OR IGNORE 防止 url 唯一约束冲突
        let mut qb =
            QueryBuilder::new("INSERT OR IGNORE INTO resource_url_info_hash (url, info_hash) ");

        qb.push_values(chunk, |mut b, item| {
            b.push_bind(&item.url).push_bind(&item.info_hash[..]);
        });

        qb.build().execute(&mut *tx).await?;
        Ok(())
    }
    pub(super) async fn batch_insert_resource(
        &self,
        tx: &mut sqlx::SqliteConnection, // 改为接收事务连接
        chunk: &[ResourceBaseData],
        need_return: bool,
    ) -> Result<Vec<ResourceProp>> {
        if chunk.is_empty() {
            return Ok(Vec::new());
        }

        let mut qb = QueryBuilder::new(
            "INSERT OR IGNORE INTO resource (info_hash, title, match_title, url, published_at) ",
        );

        qb.push_values(chunk, |mut b, item| {
            b.push_bind(&item.info_hash[..])
                .push_bind(&item.title)
                .push_bind(&item.match_title)
                .push_bind(&item.url)
                .push_bind(item.published_at);
        });

        if need_return {
            qb.push(" RETURNING info_hash, title, match_title, url, published_at");
            // 使用 &mut *tx 执行查询
            let rows = qb.build().fetch_all(&mut *tx).await?;
            let mut props = Vec::with_capacity(rows.len());
            for row in rows {
                props.push(ResourceProp {
                    data: Self::parse_resource_row(&row)?,
                });
            }
            Ok(props)
        } else {
            qb.build().execute(&mut *tx).await?;
            Ok(Vec::new())
        }
    }
}
