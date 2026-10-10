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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::cap::ResourceRepository;
    use sqlx::sqlite::SqliteConnectOptions;

    // 真实 info_hash A：取自
    // https://archive.org/download/100200300_359/100200300_359_archive.torrent 的 info 段 SHA-1
    const HASH_A: [u8; 20] = [
        0xcf, 0x8c, 0xd6, 0xac, 0x7f, 0x30, 0xed, 0x26, 0x2d, 0xdc, 0x42, 0x00, 0x9a, 0xec, 0x0d,
        0x53, 0x0e, 0x6c, 0xf7, 0x1f,
    ];

    // 真实 info_hash B：取自 https://nyaa.si/?page=rss 首条记录的 nyaa:infoHash
    const HASH_B: [u8; 20] = [
        0xd1, 0x8e, 0x3d, 0x44, 0x91, 0xdd, 0xae, 0xe3, 0x33, 0xcc, 0x23, 0x78, 0x6e, 0xa2, 0x64,
        0xc1, 0xea, 0xc2, 0x25, 0xcc,
    ];

    // 使用 tempfile::tempdir() 创建独立的临时 SQLite 库，TempDir 释放时自动清理
    async fn setup() -> (tempfile::TempDir, ResourceSqliteClient) {
        let dir = tempfile::tempdir().expect("create temp dir failed");
        let db_path = dir.path().join("resource_test.db");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = Pool::<Sqlite>::connect_with(options)
            .await
            .expect("connect temp SQLite failed");
        (dir, ResourceSqliteClient::new(pool))
    }

    fn resource_data(hash: [u8; 20], title: &str) -> ResourceBaseData {
        ResourceBaseData {
            title: title.to_string(),
            match_title: title.to_lowercase(),
            url: format!("https://nyaa.si/download/{}.torrent", hex::encode(hash)),
            info_hash: hash,
            published_at: 1_790_931_226,
        }
    }

    #[tokio::test]
    async fn test_init_with_tx_creates_schema_and_parse_resource_row() {
        let (dir, client) = setup().await;

        let mut tx = client.pool.begin().await.expect("begin transaction failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("create table should succeed");

        let data = resource_data(HASH_A, "葬送的芙莉莲 第01话");
        sqlx::query(
            "INSERT INTO resource (info_hash, title, match_title, url, published_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&data.info_hash[..])
        .bind(&data.title)
        .bind(&data.match_title)
        .bind(&data.url)
        .bind(data.published_at)
        .execute(&mut *tx)
        .await
        .expect("insert resource failed");

        tx.commit().await.expect("commit transaction failed");

        // 通过真实 SQLite 行还原 ResourceBaseData
        let row = sqlx::query(
            "SELECT info_hash, title, match_title, url, published_at FROM resource WHERE info_hash = ?",
        )
        .bind(&HASH_A[..])
        .fetch_one(&client.pool)
        .await
        .expect("query resource failed");

        let parsed = ResourceSqliteClient::parse_resource_row(&row).expect("parse row failed");
        assert_eq!(parsed.info_hash, HASH_A);
        assert_eq!(parsed.title, "葬送的芙莉莲 第01话");
        assert_eq!(parsed.match_title, "葬送的芙莉莲 第01话".to_lowercase());
        assert_eq!(parsed.url, data.url);
        assert_eq!(parsed.published_at, 1_790_931_226);

        assert!(dir.path().join("resource_test.db").exists());
    }

    #[tokio::test]
    async fn test_parse_resource_row_rejects_short_info_hash() {
        let (dir, client) = setup().await;

        let mut tx = client.pool.begin().await.expect("begin transaction failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("create table should succeed");

        // 16 字节的 info_hash 不是合法的 btih，解析时必须报错而不是 panic
        sqlx::query(
            "INSERT INTO resource (info_hash, title, match_title, url, published_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(vec![0_u8; 16])
        .bind("标题")
        .bind("标题")
        .bind("https://nyaa.si/download/bad.torrent")
        .bind(0_i64)
        .execute(&mut *tx)
        .await
        .expect("insert resource failed");

        tx.commit().await.expect("commit transaction failed");

        let row =
            sqlx::query("SELECT info_hash, title, match_title, url, published_at FROM resource")
                .fetch_one(&client.pool)
                .await
                .expect("query resource failed");

        let err = ResourceSqliteClient::parse_resource_row(&row)
            .expect_err("short info_hash should fail");
        assert!(err.to_string().contains("info_hash length is not 20"));

        assert!(dir.path().join("resource_test.db").exists());
    }

    #[tokio::test]
    async fn test_batch_insert_resource_only_returns_new_rows() {
        let (dir, client) = setup().await;

        let mut tx = client.pool.begin().await.expect("begin transaction failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("create table should succeed");

        let items = vec![
            resource_data(HASH_A, "第一条资源"),
            resource_data(HASH_B, "第二条资源"),
        ];

        let props = client
            .batch_insert_resource(&mut tx, &items, true)
            .await
            .expect("batch insert failed");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].data.info_hash, HASH_A);
        assert_eq!(props[1].data.info_hash, HASH_B);

        // 相同主键再次写入会被 INSERT OR IGNORE 忽略，不返回新行
        let again = client
            .batch_insert_resource(&mut tx, &items, true)
            .await
            .expect("duplicate batch insert failed");
        assert!(again.is_empty());

        // need_return = false 时不返回数据
        let without_return = client
            .batch_insert_resource(&mut tx, &items, false)
            .await
            .expect("batch insert without return failed");
        assert!(without_return.is_empty());

        tx.commit().await.expect("commit transaction failed");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resource")
            .fetch_one(&client.pool)
            .await
            .expect("count rows failed");
        assert_eq!(count, 2);

        assert!(dir.path().join("resource_test.db").exists());
    }

    #[tokio::test]
    async fn test_find_by_info_hashes_only_returns_stored_rows() {
        let (dir, client) = setup().await;

        let mut tx = client.pool.begin().await.expect("begin transaction failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("create table should succeed");

        let items = vec![
            resource_data(HASH_A, "第一条资源"),
            resource_data(HASH_B, "第二条资源"),
        ];
        client
            .batch_insert_resource(&mut tx, &items, false)
            .await
            .expect("batch insert failed");
        tx.commit().await.expect("commit transaction failed");

        // 库里没有这个 info_hash
        let mut missing = HASH_A;
        missing[0] ^= 0xff;

        let found = client
            .find_by_info_hashes(&[missing, HASH_B, HASH_A])
            .await
            .expect("find resources failed");
        let mut hashes: Vec<[u8; 20]> = found.iter().map(|prop| prop.data.info_hash).collect();
        hashes.sort_unstable();
        let mut expected = vec![HASH_A, HASH_B];
        expected.sort_unstable();
        assert_eq!(
            hashes, expected,
            "find_by_info_hashes should only return stored info_hash"
        );
        assert!(
            found.iter().all(|prop| !prop.data.url.is_empty()),
            "find_by_info_hashes should return the whole row because binding needs url"
        );

        let empty = client
            .find_by_info_hashes(&[])
            .await
            .expect("empty info hashes failed");
        assert!(
            empty.is_empty(),
            "find_by_info_hashes should return empty for empty input"
        );

        let none = client
            .find_by_info_hashes(&[missing])
            .await
            .expect("missing info hash failed");
        assert!(
            none.is_empty(),
            "find_by_info_hashes should return empty when nothing is stored"
        );

        assert!(dir.path().join("resource_test.db").exists());
    }

    #[tokio::test]
    async fn test_batch_insert_url_hash_skips_duplicated_url() {
        let (dir, client) = setup().await;

        let mut tx = client.pool.begin().await.expect("begin transaction failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("create table should succeed");

        let items = vec![resource_data(HASH_A, "第一条资源")];
        client
            .batch_insert_url_hash(&mut tx, &items)
            .await
            .expect("insert url mapping failed");
        // url 唯一约束冲突时应当被忽略
        client
            .batch_insert_url_hash(&mut tx, &items)
            .await
            .expect("duplicate url mapping insert failed");

        tx.commit().await.expect("commit transaction failed");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resource_url_info_hash")
            .fetch_one(&client.pool)
            .await
            .expect("count rows failed");
        assert_eq!(count, 1);

        assert!(dir.path().join("resource_test.db").exists());
    }
}
