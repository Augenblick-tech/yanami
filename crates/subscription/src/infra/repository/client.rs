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
    pub(super) const EPISODE_SELECT_JOIN: &str = r#"SELECT
        se.id,
        se.sub_anime_id,
        se.resource_id,
        se.status,
        se.ep_num,
        r.title,
        r.url,
        sa.space_id,
        ase.season_number AS season,
        at.name AS anime_origin_title
    FROM sub_anime_episode se
    JOIN resource r ON r.info_hash = se.resource_id
    JOIN sub_anime sa ON sa.id = se.sub_anime_id
    LEFT JOIN anime_season ase ON ase.anime_id = sa.anime_id AND ase.target_source = 'Bangumi'
    LEFT JOIN anime_title at ON at.anime_id = sa.anime_id AND at.is_origin = 1"#;

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
        let season: u32 = row.try_get("season")?;
        let space_id: i64 = row.try_get("space_id")?;
        let anime_origin_title: String = row.try_get("anime_origin_title")?;

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
                season,
                anime_origin_title,
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
