use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use sqlx::{Pool, Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::{
    entity::model::{
        Episode, EpisodeBaseData, EpisodeExtendData, EpisodeProp, EpsiodeStatus, Mandate, Rule,
        RuleBaseData, SearchMandateBaseData, SearchMandateProp, SubAnimeBaseData,
        SubAnimeExtendData, SubAnimeProps, SubAnimeSearchStatus,
    },
    infra::regex::RegexRuleMatcher,
};

#[derive(Clone)]
pub struct SubAnimeSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl SubAnimeSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

impl SubAnimeSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS sub_anime (
                id              INTEGER PRIMARY KEY NOT NULL,
                anime_id        INTEGER NOT NULL,
                space_id        INTEGER NOT NULL,
                rule_id         INTEGER NULL,
                search_status   INTEGER NOT NULL DEFAULT 0,
                progress        INTEGER NOT NULL DEFAULT 0,
                created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                CONSTRAINT uk_space_anime UNIQUE (space_id, anime_id)
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sub_anime_space ON sub_anime(space_id);")
            .execute(&mut **tx)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sub_anime_rule_id ON sub_anime(rule_id);")
            .execute(&mut **tx)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sub_anime_search_status ON sub_anime(search_status) WHERE search_status > 0;",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS sub_anime_episode (
                id              INTEGER PRIMARY KEY NOT NULL,
                sub_anime_id    INTEGER NOT NULL,
                resource_id     BLOB NOT NULL,
                status          INTEGER NOT NULL DEFAULT 0,
                ep_num          REAL NULL,
                created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                CONSTRAINT uk_sub_anime_resource UNIQUE (sub_anime_id, resource_id)
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_episode_pending ON sub_anime_episode(status) WHERE status = 0;",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_episode_sub_anime_id ON sub_anime_episode(sub_anime_id);")
            .execute(&mut **tx)
            .await?;

        Ok(())
    }
}

impl SubAnimeSqliteClient {
    /// 剧集落点（系列目录 / 季号）由领域层的系列负责，这里只把订阅侧要用的字段取齐：
    /// 剧集自身、资源信息、以及所属番剧 id（落点要用它去问系列）。
    pub(super) const EPISODE_SELECT_JOIN: &str = r#"SELECT
        se.id,
        se.sub_anime_id,
        se.resource_id,
        se.status,
        se.ep_num,
        r.title,
        r.url,
        sa.anime_id,
        sa.space_id
    FROM sub_anime_episode se
    JOIN resource r ON r.info_hash = se.resource_id
    JOIN sub_anime sa ON sa.id = se.sub_anime_id"#;

    pub(super) fn row_to_episode_prop(row: &sqlx::sqlite::SqliteRow) -> Result<EpisodeProp> {
        let id: i64 = row.try_get("id")?;
        let sub_anime_id: i64 = row.try_get("sub_anime_id")?;
        let resource_blob: Vec<u8> = row.try_get("resource_id")?;
        let resource_id: [u8; 20] = resource_blob
            .try_into()
            .map_err(|_| anyhow!("invalid resource_id length: expected 20"))?;

        let status: i32 = row.try_get("status")?;
        let status = EpsiodeStatus::try_from(status).map_err(|e| anyhow!("{}", e))?;

        let ep_num: Option<f64> = row.try_get("ep_num")?;

        let title: String = row.try_get("title")?;
        let url: String = row.try_get("url")?;
        let anime_id: i64 = row.try_get("anime_id")?;
        let space_id: i64 = row.try_get("space_id")?;

        Ok(EpisodeProp {
            data: EpisodeBaseData {
                id,
                ep: Episode {
                    sub_anime_id,
                    resource_id,
                    status,
                    ep_num,
                },
            },
            extend: EpisodeExtendData {
                title,
                url,
                anime_id,
                space_id,
            },
        })
    }
}

impl SubAnimeSqliteClient {
    pub(super) const BASE_SELECT_JOIN: &str = r#"SELECT
        sa.id,
        sa.anime_id,
        sa.space_id,
        sa.rule_id,
        sa.search_status,
        sa.progress,
        COALESCE(
            (SELECT planned_ep_count FROM anime_season
             WHERE anime_id = sa.anime_id AND target_source = 'Bangumi'),
            0
        ) AS eps,
        r.name AS rule_name,
        a.air_date,
        COALESCE(
            json_group_array(at.name) FILTER (WHERE at.name IS NOT NULL),
            '[]'
        ) AS titles_json
    FROM sub_anime sa
    JOIN anime a ON a.id = sa.anime_id
    LEFT JOIN rule r ON r.id = sa.rule_id
    LEFT JOIN anime_title at ON at.anime_id = sa.anime_id"#;

    pub(super) fn row_to_sub_anime_props(row: &sqlx::sqlite::SqliteRow) -> Result<SubAnimeProps> {
        let search_status: i32 = row.try_get("search_status")?;
        let search_status =
            SubAnimeSearchStatus::try_from(search_status).map_err(|e| anyhow::anyhow!("{}", e))?;

        let base_data = SubAnimeBaseData {
            id: row.try_get("id")?,
            anime_id: row.try_get("anime_id")?,
            space_id: row.try_get("space_id")?,
            rule_id: row.try_get("rule_id")?,
            search_status,
            progress: row.try_get::<i32, _>("progress")? as u32,
        };

        let air_date_str: String = row.try_get("air_date")?;
        let air_date = NaiveDate::parse_from_str(&air_date_str, "%Y-%m-%d")
            .context("failed to parse air_date")?;

        let eps: u32 = row.try_get::<i32, _>("eps")? as u32;
        let rule_name: Option<String> = row.try_get("rule_name")?;

        let titles_json: String = row.try_get("titles_json")?;
        let titles: Vec<String> =
            serde_json::from_str(&titles_json).context("failed to parse titles json")?;

        Ok(SubAnimeProps {
            data: base_data,
            extend: SubAnimeExtendData {
                eps,
                rule_name,
                titles,
                air_date,
            },
        })
    }
}

#[derive(Clone)]
pub struct RuleSqliteClient {
    pub(super) pool: Pool<Sqlite>,
    pub regex_cache: RegexRuleMatcher,
}

impl RuleSqliteClient {
    pub fn new(pool: Pool<Sqlite>, matcher: RegexRuleMatcher) -> Self {
        Self {
            pool,
            regex_cache: matcher,
        }
    }
}

impl RuleSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS rule (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                `order`       INTEGER NOT NULL,
                space_id    INTEGER NOT NULL,
                pattern     TEXT NOT NULL,
                deleted_at  INTEGER
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rule_space_id ON rule(space_id);")
            .execute(&mut **tx)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_rule_deleted_at ON rule(deleted_at);")
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}

impl RuleSqliteClient {
    pub(super) fn parse_raw(row: &SqliteRow) -> Result<RuleBaseData, sqlx::Error> {
        let id: i64 = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let order: i64 = row.try_get("order")?;
        let space_id: i64 = row.try_get("space_id")?;
        let pattern: String = row.try_get("pattern")?;
        let deleted_at: Option<i64> = row.try_get("deleted_at")?;

        Ok(RuleBaseData {
            id,
            active: deleted_at.is_none(),
            metadata: Rule {
                space_id,
                name,
                order,
                pattern,
            },
        })
    }
}

#[derive(Clone)]
pub struct SearchMandateSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl SearchMandateSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

impl SearchMandateSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS search_mandate (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,  
                anime_id    INTEGER NOT NULL UNIQUE
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_search_mandate_anime_id ON search_mandate(anime_id);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS search_pool (
                id                      INTEGER PRIMARY KEY AUTOINCREMENT,  
                search_mandate_id       INTEGER NOT NULL,
                feed_id                 INTEGER NOT NULL,
                url                     TEXT NOT NULL
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_search_pool_feed_id ON search_pool(feed_id);")
            .execute(&mut **tx)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_search_pool_search_mandate_id ON search_pool(search_mandate_id);")
            .execute(&mut **tx)
            .await?;

        Ok(())
    }
}

impl SearchMandateSqliteClient {
    pub(super) fn parse_row(row: &sqlx::sqlite::SqliteRow) -> Result<SearchMandateProp> {
        let id: i64 = row.try_get("id").context("missing column 'id'")?;
        let anime_id: i64 = row
            .try_get("anime_id")
            .context("missing column 'anime_id'")?;
        let feed_id: i64 = row.try_get("feed_id").context("missing column 'feed_id'")?;
        let url: String = row.try_get("url").context("missing column 'url'")?;

        Ok(SearchMandateProp {
            data: SearchMandateBaseData {
                id,
                mandata: Mandate {
                    anime_id,
                    feed_id,
                    url,
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dashmap::DashMap;
    use regex::Regex;
    use sqlx::SqlitePool;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use crate::entity::cap::{RuleRepository, SubAnimeRepository};
    use crate::entity::model::{Episode, EpsiodeStatus, Rule};
    use crate::infra::regex::RegexRuleMatcher;

    use super::{RuleSqliteClient, SearchMandateSqliteClient, SubAnimeSqliteClient};

    struct Fixture {
        dir: tempfile::TempDir,
        pool: SqlitePool,
    }

    async fn setup() -> Fixture {
        let dir = tempfile::tempdir().expect("create temp dir failed");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(dir.path().join("client-test.db"))
                    .create_if_missing(true),
            )
            .await
            .expect("connect sqlite failed");

        let sub_client = SubAnimeSqliteClient::new(pool.clone());
        let rule_client = RuleSqliteClient::new(
            pool.clone(),
            RegexRuleMatcher::new(Arc::new(DashMap::<String, Regex>::new())),
        );
        let mandate_client = SearchMandateSqliteClient::new(pool.clone());

        let mut tx = pool.begin().await.expect("begin schema tx failed");
        sub_client
            .init_with_tx(&mut tx)
            .await
            .expect("init sub_anime schema failed");
        rule_client
            .init_with_tx(&mut tx)
            .await
            .expect("init rule schema failed");
        mandate_client
            .init_with_tx(&mut tx)
            .await
            .expect("init search mandate schema failed");
        // subscription 不依赖 anime / resource crate，测试里按生产查询实际引用的列最小化建表
        sqlx::query("CREATE TABLE anime (id INTEGER PRIMARY KEY, air_date TEXT);")
            .execute(&mut *tx)
            .await
            .expect("create anime table failed");
        sqlx::query(
            "CREATE TABLE anime_title (id INTEGER PRIMARY KEY AUTOINCREMENT, anime_id INTEGER NOT NULL, name TEXT NOT NULL);",
        )
        .execute(&mut *tx)
        .await
        .expect("create anime_title table failed");
        sqlx::query(
            "CREATE TABLE anime_season (id INTEGER PRIMARY KEY AUTOINCREMENT, anime_id INTEGER NOT NULL, target_source TEXT NOT NULL, planned_ep_count INTEGER NOT NULL);",
        )
        .execute(&mut *tx)
        .await
        .expect("create anime_season table failed");
        sqlx::query(
            "CREATE TABLE resource (info_hash BLOB NOT NULL PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL);",
        )
        .execute(&mut *tx)
        .await
        .expect("create resource table failed");
        tx.commit().await.expect("commit schema failed");

        Fixture { dir, pool }
    }

    async fn seed_anime(
        pool: &SqlitePool,
        anime_id: i64,
        air_date: Option<&str>,
        planned_eps: Option<i64>,
        titles: &[&str],
    ) {
        let mut tx = pool.begin().await.expect("begin seed anime tx failed");
        sqlx::query("INSERT INTO anime (id, air_date) VALUES (?, ?)")
            .bind(anime_id)
            .bind(air_date)
            .execute(&mut *tx)
            .await
            .expect("insert anime failed");
        if let Some(planned) = planned_eps {
            sqlx::query(
                "INSERT INTO anime_season (anime_id, target_source, planned_ep_count) VALUES (?, 'Bangumi', ?)",
            )
            .bind(anime_id)
            .bind(planned)
            .execute(&mut *tx)
            .await
            .expect("insert anime_season failed");
        }
        for title in titles {
            sqlx::query("INSERT INTO anime_title (anime_id, name) VALUES (?, ?)")
                .bind(anime_id)
                .bind(title)
                .execute(&mut *tx)
                .await
                .expect("insert anime_title failed");
        }
        tx.commit().await.expect("commit seed anime failed");
    }

    async fn seed_resource(pool: &SqlitePool, hash: &[u8], title: &str, url: &str) {
        sqlx::query("INSERT INTO resource (info_hash, title, url) VALUES (?, ?, ?)")
            .bind(hash)
            .bind(title)
            .bind(url)
            .execute(pool)
            .await
            .expect("insert resource failed");
    }

    /// 走生产的 BASE_SELECT_JOIN（每个用例只造一条 sub_anime，直接 fetch_one）
    async fn fetch_sub_anime_row(pool: &SqlitePool) -> sqlx::sqlite::SqliteRow {
        sqlx::query(SubAnimeSqliteClient::BASE_SELECT_JOIN)
            .fetch_one(pool)
            .await
            .expect("fetch sub anime row failed")
    }

    #[tokio::test]
    async fn fixture_database_file_lives_under_tempdir() {
        // 夹具把 SQLite 文件放在 tempdir 下，测试结束随目录一起删除
        let f = setup().await;
        assert!(f.dir.path().join("client-test.db").exists());
    }

    #[tokio::test]
    async fn row_to_sub_anime_props_reads_all_columns() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A", "A番"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let inserted = client
            .insert_sub_anime(9, 100)
            .await
            .expect("insert sub anime");

        let row = fetch_sub_anime_row(&f.pool).await;
        let props = SubAnimeSqliteClient::row_to_sub_anime_props(&row).expect("parse row");

        assert_eq!(props.data.id, inserted.data.id);
        assert_eq!(props.data.anime_id, 100);
        assert_eq!(props.data.space_id, 9);
        assert_eq!(props.data.rule_id, None);
        assert_eq!(
            props.data.search_status,
            crate::entity::model::SubAnimeSearchStatus::Pending
        );
        assert_eq!(props.data.progress, 0);
        assert_eq!(props.extend.eps, 12);
        assert_eq!(props.extend.rule_name, None);
        assert_eq!(
            props.extend.air_date,
            chrono::NaiveDate::from_ymd_opt(2024, 4, 1).expect("valid date")
        );
        let mut titles = props.extend.titles.clone();
        titles.sort();
        assert_eq!(titles, vec!["A番".to_string(), "番A".to_string()]);
    }

    #[tokio::test]
    async fn row_to_sub_anime_props_defaults_titles_to_empty_array() {
        let f = setup().await;
        // 没有 anime_title 行时 json_group_array 聚合出 '[]'
        seed_anime(&f.pool, 200, Some("2024-05-01"), Some(6), &[]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        client
            .insert_sub_anime(9, 200)
            .await
            .expect("insert sub anime");

        let row = fetch_sub_anime_row(&f.pool).await;
        let props = SubAnimeSqliteClient::row_to_sub_anime_props(&row).expect("parse row");

        assert!(props.extend.titles.is_empty());
        assert_eq!(props.extend.eps, 6);
    }

    #[tokio::test]
    async fn row_to_sub_anime_props_reports_null_air_date_known_defect() {
        // 已知缺陷（client.rs:190-192）：anime.air_date 在 anime 表里本来允许为 NULL，
        // 但这里无条件把 air_date 解成 String 再 parse_from_str，NULL 走不通。
        // 更糟的是 insert_sub_anime → find_sub_anime 内部也调它，
        // 于是 air_date 缺失的番剧连订阅都建不起来（INSERT 已经落库，调用方却拿到 Err）。
        // 本用例锁定当前行为，不做修复。
        let f = setup().await;
        seed_anime(&f.pool, 300, None, Some(12), &["番C"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());

        let insert_err = client
            .insert_sub_anime(9, 300)
            .await
            .expect_err("NULL air_date must fail today");
        assert!(
            insert_err.to_string().contains("failed to parse air_date"),
            "unexpected error: {insert_err}"
        );

        // 行已经写进去了，直接按生产查询取出来，单独验证解析函数本身
        let row = fetch_sub_anime_row(&f.pool).await;
        let err = SubAnimeSqliteClient::row_to_sub_anime_props(&row)
            .expect_err("NULL air_date must fail today");
        assert!(
            err.to_string().contains("failed to parse air_date"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn row_to_sub_anime_props_reports_empty_air_date_known_defect() {
        // 已知缺陷（client.rs:190-192）：air_date 空串能解码成 String，
        // 但 NaiveDate::parse_from_str("") 必然失败，同样让整个 list 查询报错。
        // 本用例锁定当前行为，不做修复。
        let f = setup().await;
        seed_anime(&f.pool, 400, Some(""), Some(12), &["番D"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());

        let insert_err = client
            .insert_sub_anime(9, 400)
            .await
            .expect_err("empty air_date must fail today");
        assert!(
            insert_err.to_string().contains("failed to parse air_date"),
            "unexpected error: {insert_err}"
        );

        let row = fetch_sub_anime_row(&f.pool).await;
        let err = SubAnimeSqliteClient::row_to_sub_anime_props(&row)
            .expect_err("empty air_date must fail today");
        assert!(
            err.to_string().contains("failed to parse air_date"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn row_to_sub_anime_props_reads_bound_rule_name() {
        let f = setup().await;
        seed_anime(&f.pool, 500, Some("2024-06-01"), Some(12), &["番E"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let inserted = client
            .insert_sub_anime(9, 500)
            .await
            .expect("insert sub anime");
        let rule_client = RuleSqliteClient::new(
            f.pool.clone(),
            RegexRuleMatcher::new(Arc::new(DashMap::<String, Regex>::new())),
        );
        let rule = rule_client
            .insert(&Rule {
                space_id: 9,
                name: "规则E".to_string(),
                order: 1,
                pattern: "番E".to_string(),
            })
            .await
            .expect("insert rule");
        sqlx::query("UPDATE sub_anime SET rule_id = ? WHERE id = ?")
            .bind(rule.id)
            .bind(inserted.data.id)
            .execute(&f.pool)
            .await
            .expect("bind rule directly");

        let row = fetch_sub_anime_row(&f.pool).await;
        let props = SubAnimeSqliteClient::row_to_sub_anime_props(&row).expect("parse row");
        assert_eq!(props.data.rule_id, Some(rule.id));
        assert_eq!(props.extend.rule_name, Some("规则E".to_string()));
    }

    async fn seed_episode(
        f: &Fixture,
        sub_anime_id: i64,
        hash: [u8; 20],
        title: &str,
        url: &str,
        status: EpsiodeStatus,
        ep_num: Option<f64>,
    ) {
        seed_resource(&f.pool, &hash, title, url).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let props = client
            .find_sub_anime(sub_anime_id)
            .await
            .expect("find sub anime")
            .expect("sub anime exists");
        let ep = Episode {
            sub_anime_id,
            resource_id: hash,
            status,
            ep_num,
        };
        client
            .update_sub_anime_progress(&props.data, &[ep])
            .await
            .expect("seed episode");
    }

    #[tokio::test]
    async fn row_to_episode_prop_reads_all_columns() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let sub = client
            .insert_sub_anime(9, 100)
            .await
            .expect("insert sub anime");
        seed_episode(
            &f,
            sub.data.id,
            [7u8; 20],
            "番A 第03集",
            "magnet:?xt=urn:btih:zzz",
            EpsiodeStatus::Downloaded,
            Some(3.5),
        )
        .await;

        let row = sqlx::query(SubAnimeSqliteClient::EPISODE_SELECT_JOIN)
            .fetch_one(&f.pool)
            .await
            .expect("fetch episode row");
        let parsed = SubAnimeSqliteClient::row_to_episode_prop(&row).expect("parse episode row");

        assert!(parsed.data.id > 0);
        assert_eq!(parsed.data.ep.sub_anime_id, sub.data.id);
        assert_eq!(parsed.data.ep.resource_id, [7u8; 20]);
        assert_eq!(parsed.data.ep.status, EpsiodeStatus::Downloaded);
        assert_eq!(parsed.data.ep.ep_num, Some(3.5));
        assert_eq!(parsed.extend.title, "番A 第03集");
        assert_eq!(parsed.extend.url, "magnet:?xt=urn:btih:zzz");
        assert_eq!(parsed.extend.anime_id, 100);
        assert_eq!(parsed.extend.space_id, 9);
    }

    #[tokio::test]
    async fn row_to_episode_prop_tolerates_null_ep_num() {
        // ep_num 是 REAL NULL，row_to_episode_prop 用 Option<f64> 解码，
        // 这里确认 NULL 不会像 air_date 那样炸掉整条查询
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let sub = client
            .insert_sub_anime(9, 100)
            .await
            .expect("insert sub anime");
        seed_episode(
            &f,
            sub.data.id,
            [8u8; 20],
            "番A 未编号",
            "magnet:?xt=urn:btih:yyy",
            EpsiodeStatus::Pending,
            None,
        )
        .await;

        let row = sqlx::query(SubAnimeSqliteClient::EPISODE_SELECT_JOIN)
            .fetch_one(&f.pool)
            .await
            .expect("fetch episode row");
        let parsed = SubAnimeSqliteClient::row_to_episode_prop(&row).expect("parse episode row");

        assert_eq!(parsed.data.ep.ep_num, None);
        assert_eq!(parsed.data.ep.status, EpsiodeStatus::Pending);
    }

    #[tokio::test]
    async fn row_to_episode_prop_rejects_bad_resource_id_length() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let client = SubAnimeSqliteClient::new(f.pool.clone());
        let sub = client
            .insert_sub_anime(9, 100)
            .await
            .expect("insert sub anime");
        // 非 20 字节的 resource_id：资源表本身不校验长度，解析时才发现
        seed_resource(&f.pool, &[1u8, 2, 3, 4], "坏资源", "http://bad").await;
        sqlx::query(
            "INSERT INTO sub_anime_episode (id, sub_anime_id, resource_id, status, ep_num) VALUES (77, ?, ?, 0, NULL)",
        )
        .bind(sub.data.id)
        .bind(vec![1u8, 2, 3, 4])
        .execute(&f.pool)
        .await
        .expect("insert bad episode");

        let row = sqlx::query(SubAnimeSqliteClient::EPISODE_SELECT_JOIN)
            .fetch_one(&f.pool)
            .await
            .expect("fetch episode row");
        let err = SubAnimeSqliteClient::row_to_episode_prop(&row)
            .expect_err("bad resource_id length must fail");
        assert!(
            err.to_string()
                .contains("invalid resource_id length: expected 20"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn parse_raw_marks_rule_active_only_without_deleted_at() {
        let f = setup().await;
        let rule_client = RuleSqliteClient::new(
            f.pool.clone(),
            RegexRuleMatcher::new(Arc::new(DashMap::<String, Regex>::new())),
        );
        let rule = rule_client
            .insert(&Rule {
                space_id: 3,
                name: "规则F".to_string(),
                order: 2,
                pattern: "番F".to_string(),
            })
            .await
            .expect("insert rule");

        let row = sqlx::query("SELECT * FROM rule WHERE id = ?")
            .bind(rule.id)
            .fetch_one(&f.pool)
            .await
            .expect("fetch rule row");
        let active = RuleSqliteClient::parse_raw(&row).expect("parse active rule");
        assert_eq!(active.id, rule.id);
        assert!(active.active);
        assert_eq!(active.metadata.space_id, 3);
        assert_eq!(active.metadata.name, "规则F");
        assert_eq!(active.metadata.order, 2);
        assert_eq!(active.metadata.pattern, "番F");

        // deleted_at 一旦有值即视为已删除
        sqlx::query("UPDATE rule SET deleted_at = 12345 WHERE id = ?")
            .bind(rule.id)
            .execute(&f.pool)
            .await
            .expect("soft delete rule");
        let row = sqlx::query("SELECT * FROM rule WHERE id = ?")
            .bind(rule.id)
            .fetch_one(&f.pool)
            .await
            .expect("fetch deleted rule row");
        let deleted = RuleSqliteClient::parse_raw(&row).expect("parse deleted rule");
        assert!(!deleted.active);
        assert_eq!(deleted.metadata.name, "规则F");
    }

    #[tokio::test]
    async fn parse_row_reads_mandate_projection() {
        let f = setup().await;
        sqlx::query("INSERT INTO search_mandate (id, anime_id) VALUES (5, 66)")
            .execute(&f.pool)
            .await
            .expect("insert mandate");
        sqlx::query(
            "INSERT INTO search_pool (id, search_mandate_id, feed_id, url) VALUES (9, 5, 77, 'http://feed/77')",
        )
        .execute(&f.pool)
        .await
        .expect("insert pool entry");

        // 与生产 get_one 相同的投影列
        let row = sqlx::query(
            "SELECT p.id, m.anime_id, p.feed_id, p.url FROM search_pool p JOIN search_mandate m ON m.id = p.search_mandate_id",
        )
        .fetch_one(&f.pool)
        .await
        .expect("fetch mandate row");
        let parsed = SearchMandateSqliteClient::parse_row(&row).expect("parse mandate row");

        assert_eq!(parsed.data.id, 9);
        assert_eq!(parsed.data.mandata.anime_id, 66);
        assert_eq!(parsed.data.mandata.feed_id, 77);
        assert_eq!(parsed.data.mandata.url, "http://feed/77");
    }
}
