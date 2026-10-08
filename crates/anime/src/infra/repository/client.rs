use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use common::{infra::app_ctx::AppContext, shared::boss::FromContext};
use sqlx::{Pool, QueryBuilder, Row, Sqlite, Transaction};

use crate::entity::model::{
    AnimeAirWeekday, AnimeBaseData, AnimeEpisode, AnimeEx, AnimeIdType, AnimeListQuery,
    AnimeMetadata, AnimeProps, AnimeSeason, AnimeSourceTarget, AnimeTitle,
};

#[derive(Clone)]
pub struct AnimeSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl AnimeSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

impl AnimeSqliteClient {
    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        // anime 元数据表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS anime (
                id              INTEGER PRIMARY KEY,                -- 对应 AnimeBaseData.id
                air_weekday     INTEGER NOT NULL,                   -- 对应 AnimeAirWeekday (1-7)，0 表示上游没给放送星期
                air_date        TEXT,                               -- 对应 NaiveDate, SQLite 标准格式 'YYYY-MM-DD'
                air_quarter     INTEGER NOT NULL,                   -- 例如 202607
                air_year        INTEGER NOT NULL,                   -- 冗余字段：从 202607 拆分，便于按年过滤
                air_month       INTEGER NOT NULL,                   -- 冗余字段：从 202607 拆分，便于按月过滤
                is_locked       INTEGER NOT NULL DEFAULT 0,         -- 对应 lock 字段，0=false, 1=true
                created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        ).execute(&mut **tx).await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_year_month ON anime(air_year, air_month);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_anime_locked ON anime(is_locked);")
            .execute(&mut **tx)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_anime_quarter ON anime(air_quarter);")
            .execute(&mut **tx)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_anime_weekday ON anime(air_weekday);")
            .execute(&mut **tx)
            .await?;

        // anime_title 番剧标题表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS anime_title (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id    INTEGER NOT NULL,
                name        TEXT NOT NULL,
                match_name  TEXT NOT NULL,
                lang_target TEXT NOT NULL,
                is_origin   INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_title_covering_name ON anime_title(anime_id, name);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_title_covering_match_name ON anime_title(anime_id, match_name);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_title_anime_id_id ON anime_title (anime_id, id);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_title_lang_target ON anime_title(lang_target);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_title_is_origin ON anime_title(is_origin);",
        )
        .execute(&mut **tx)
        .await?;

        // 创建 FTS5 虚拟表用于极速全文检索
        // content='': 开启无内容模式，节约空间
        // contentless_delete=1: 允许删除无内容表中的记录
        // tokenize='unicode61': 支持 Unicode 分词
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS anime_alias_fts USING fts5(
                char_text,
                content='',
                contentless_delete=1,
                tokenize='unicode61'
            );",
        )
        .execute(&mut **tx)
        .await?;

        // anime_external 番剧外部信息关联表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS anime_external (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id        INTEGER NOT NULL,
                target_source   TEXT NOT NULL,
                ext_type        TEXT,
                ext_id          TEXT,
                created_at      INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_ext_anime_id ON anime_external(anime_id);",
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_anime_ext_lookup ON anime_external(target_source, ext_id);")
        .execute(&mut **tx)
        .await?;

        // anime_season 番剧季度信息表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS anime_season (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id            INTEGER NOT NULL,
                target_source       TEXT NOT NULL,
                lang_target         TEXT NOT NULL,
                season_number       INTEGER NOT NULL,
                planned_ep_count    INTEGER NOT NULL,
                description         TEXT NOT NULL DEFAULT '',
                created_at          INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at          INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_season_anime_id_sort ON anime_season (anime_id, season_number, id);",
        )
        .execute(&mut **tx)
        .await?;

        // anime_epsiode 番剧剧集信息表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS anime_episode (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id INTEGER NOT NULL,
                season_id INTEGER NOT NULL,
                titles TEXT NOT NULL, -- JSON 类型数据
                ep_number INTEGER NOT NULL,
                sort_number REAL NOT NULL,
                air_date TEXT,
                duration_seconds INTEGER NOT NULL DEFAULT 0,
                description TEXT NOT NULL DEFAULT '',
                ext_id TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_anime_ep_anime_id ON anime_episode(anime_id);")
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_anime_ep_season_id ON anime_episode(season_id);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS anime_series (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                origin_name     TEXT NOT NULL,
                cn_name         TEXT NOT NULL DEFAULT '',
                air_date        TEXT NOT NULL,
                description     TEXT NOT NULL DEFAULT '',
                genres          TEXT,
                tmdb_id         INTEGER NOT NULL,
                created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
            );
        ",
        )
        .execute(&mut **tx)
        .await?;

        // tmdb_id 就是系列的唯一身份：先去重再建唯一索引，保证一个剧集只有一行系列信息
        sqlx::query(
            "DELETE FROM anime_series WHERE id NOT IN (SELECT MAX(id) FROM anime_series GROUP BY tmdb_id);",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_anime_series_tmdb_id ON anime_series(tmdb_id);",
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

impl AnimeSqliteClient {
    pub(super) fn parse_anime_row(row: &sqlx::sqlite::SqliteRow) -> Result<AnimeProps> {
        let anime_id: i64 = row
            .try_get("anime_id")
            .context("Missing column 'anime_id'")?;

        // 系列信息来自 TMDB 系列身份的联表：联不上就是没有系列，不编造空值
        let series_metadata = match row.try_get::<Option<String>, _>("series_origin_name")? {
            Some(origin_name) => {
                let cn_name: String = row
                    .try_get("series_cn_name")
                    .with_context(|| format!("Anime {} missing 'series_cn_name'", anime_id))?;

                let desc: String = row
                    .try_get("series_desc")
                    .with_context(|| format!("Anime {} missing 'series_desc'", anime_id))?;

                let series_air_date_str: String = row
                    .try_get("series_air_date")
                    .with_context(|| format!("Anime {} missing 'series_air_date'", anime_id))?;
                let air_date = NaiveDate::parse_from_str(&series_air_date_str, "%Y-%m-%d")
                    .with_context(|| {
                        format!(
                            "Anime {} has invalid series_air_date format: {}",
                            anime_id, series_air_date_str
                        )
                    })?;

                let series_genres_json: String = row
                    .try_get("series_genres")
                    .with_context(|| format!("Anime {} missing 'series_genres'", anime_id))?;
                let genres: Vec<String> =
                    serde_json::from_str(&series_genres_json).with_context(|| {
                        format!("Anime {} series_genres JSON parse failed", anime_id)
                    })?;

                Some(crate::entity::model::AnimeSeriesMetadata {
                    origin_name,
                    cn_name,
                    desc,
                    air_date,
                    genres,
                })
            }
            None => None,
        };

        // 1. 基础属性解析
        let weekday_i64: i64 = row
            .try_get("air_weekday")
            .with_context(|| format!("Anime {} missing 'air_weekday'", anime_id))?;
        // 放送星期允许为空，0 表示没写
        let air_weekday = AnimeAirWeekday::try_from(weekday_i64).ok();

        let air_date_str: String = row
            .try_get("air_date")
            .with_context(|| format!("Anime {} missing 'air_date'", anime_id))?;
        let air_date = NaiveDate::parse_from_str(&air_date_str, "%Y-%m-%d").with_context(|| {
            format!(
                "Anime {} has invalid air_date format: {}",
                anime_id, air_date_str
            )
        })?;

        let air_quarter: u32 = row
            .try_get::<i32, _>("air_quarter")
            .with_context(|| format!("Anime {} missing 'air_quarter'", anime_id))?
            as u32;

        let lock: bool = row
            .try_get::<i64, _>("is_locked")
            .with_context(|| format!("Anime {} missing 'is_locked'", anime_id))?
            != 0;

        // 2. 严格解析标题 JSON
        let titles_json: String = row
            .try_get("titles")
            .with_context(|| format!("Anime {} missing 'titles' column", anime_id))?;
        let titles_val: serde_json::Value = serde_json::from_str(&titles_json)
            .with_context(|| format!("Anime {} titles JSON parse failed", anime_id))?;

        let mut titles = Vec::new();
        let titles_arr = titles_val
            .as_array()
            .ok_or_else(|| anyhow!("Anime {} titles is not a JSON array", anime_id))?;

        for (i, v) in titles_arr.iter().enumerate() {
            titles.push(AnimeTitle {
                name: v["name"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Anime {} titles[{}] missing 'name'", anime_id, i))?
                    .to_string(),
                match_name: v["match_name"]
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!("Anime {} titles[{}] missing 'match_name'", anime_id, i)
                    })?
                    .to_string(),
                target: v["lang_target"]
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!("Anime {} titles[{}] missing 'lang_target'", anime_id, i)
                    })?
                    .into(),
                origin: v["is_origin"].as_i64().ok_or_else(|| {
                    anyhow!("Anime {} titles[{}] missing 'is_origin'", anime_id, i)
                })? != 0,
            });
        }

        // 3. 严格解析外部链接 JSON
        let ext_json: String = row
            .try_get("external_links")
            .with_context(|| format!("Anime {} missing 'external_links' column", anime_id))?;
        let ext_val: serde_json::Value = serde_json::from_str(&ext_json)
            .with_context(|| format!("Anime {} external_links JSON parse failed", anime_id))?;

        let mut external_link = Vec::new();
        let ext_arr = ext_val
            .as_array()
            .ok_or_else(|| anyhow!("Anime {} external_links is not a JSON array", anime_id))?;

        for (i, v) in ext_arr.iter().enumerate() {
            let source_str = v["target_source"].as_str().ok_or_else(|| {
                anyhow!(
                    "Anime {} ext_links[{}] missing 'target_source'",
                    anime_id,
                    i
                )
            })?;

            let target: AnimeSourceTarget = source_str.into();

            let ext_id_str = v["ext_id"]
                .as_str()
                .ok_or_else(|| anyhow!("Anime {} ext_links[{}] missing 'ext_id'", anime_id, i))?
                .to_string();

            let id = ext_id_str
                .parse::<i64>()
                .map(AnimeIdType::Int)
                .unwrap_or_else(|_| AnimeIdType::String(ext_id_str));

            // ext_type 业务上是 Option，这里可以用 map 处理 null 的情况
            let ext_type = v["ext_type"].as_str().map(|s| s.to_string());

            external_link.push(AnimeEx {
                id,
                target,
                r#type: ext_type,
            });
        }

        // 4. 严格解析季度和剧集 JSON
        let seasons_json: String = row
            .try_get("seasons")
            .with_context(|| format!("Anime {} missing 'seasons' column", anime_id))?;
        let seasons_val: serde_json::Value = serde_json::from_str(&seasons_json)
            .with_context(|| format!("Anime {} seasons JSON parse failed", anime_id))?;

        let mut season = Vec::new();
        let season_arr = seasons_val
            .as_array()
            .ok_or_else(|| anyhow!("Anime {} seasons is not a JSON array", anime_id))?;

        for (s_i, s_val) in season_arr.iter().enumerate() {
            let source_str = s_val["target_source"].as_str().ok_or_else(|| {
                anyhow!("Anime {} season[{}] missing 'target_source'", anime_id, s_i)
            })?;

            let target: AnimeSourceTarget = source_str.into();

            let mut eps = Vec::new();
            let eps_arr = s_val["eps"].as_array().ok_or_else(|| {
                anyhow!("Anime {} season[{}] eps is not a JSON array", anime_id, s_i)
            })?;

            for (ep_i, ep_val) in eps_arr.iter().enumerate() {
                let ep_date_str = ep_val["air_date"].as_str().ok_or_else(|| {
                    anyhow!(
                        "Anime {} season[{}] ep[{}] missing 'air_date'",
                        anime_id,
                        s_i,
                        ep_i
                    )
                })?;
                let ep_air_date =
                    NaiveDate::parse_from_str(ep_date_str, "%Y-%m-%d").with_context(|| {
                        format!(
                            "Anime {} season[{}] ep[{}] invalid date format",
                            anime_id, s_i, ep_i
                        )
                    })?;

                let ext_id_str = ep_val["ext_id"]
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!(
                            "Anime {} season[{}] ep[{}] missing 'ext_id'",
                            anime_id,
                            s_i,
                            ep_i
                        )
                    })?
                    .to_string();
                let ex_id = ext_id_str
                    .parse::<i64>()
                    .map(AnimeIdType::Int)
                    .unwrap_or_else(|_| AnimeIdType::String(ext_id_str));

                // titles 是已严格定义的序列化结构，直接反序列化，失败则暴雷
                let ep_titles: Vec<AnimeTitle> = serde_json::from_value(ep_val["titles"].clone())
                    .with_context(|| {
                    format!(
                        "Anime {} season[{}] ep[{}] titles mapping failed",
                        anime_id, s_i, ep_i
                    )
                })?;

                eps.push(AnimeEpisode {
                    ep: ep_val["ep_number"].as_u64().ok_or_else(|| {
                        anyhow!(
                            "Anime {} season[{}] ep[{}] missing 'ep_number'",
                            anime_id,
                            s_i,
                            ep_i
                        )
                    })? as u32,
                    sort: ep_val["sort_number"].as_f64().ok_or_else(|| {
                        anyhow!(
                            "Anime {} season[{}] ep[{}] missing 'sort_number'",
                            anime_id,
                            s_i,
                            ep_i
                        )
                    })?,
                    air_date: ep_air_date,
                    title: ep_titles,
                    duration_seconds: ep_val["duration_seconds"].as_u64().ok_or_else(|| {
                        anyhow!(
                            "Anime {} season[{}] ep[{}] missing 'duration_seconds'",
                            anime_id,
                            s_i,
                            ep_i
                        )
                    })?,
                    desc: ep_val["description"]
                        .as_str()
                        .ok_or_else(|| {
                            anyhow!(
                                "Anime {} season[{}] ep[{}] missing 'description'",
                                anime_id,
                                s_i,
                                ep_i
                            )
                        })?
                        .to_string(),
                    ex_id,
                });
            }

            season.push(AnimeSeason {
                target,
                lang: s_val["lang_target"]
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!("Anime {} season[{}] missing 'lang_target'", anime_id, s_i)
                    })?
                    .into(),
                desc: s_val["description"]
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!("Anime {} season[{}] missing 'description'", anime_id, s_i)
                    })?
                    .to_string(),
                season: s_val["season_number"].as_u64().ok_or_else(|| {
                    anyhow!("Anime {} season[{}] missing 'season_number'", anime_id, s_i)
                })? as u32,
                planned_episode_count: s_val["planned_ep_count"].as_u64().ok_or_else(|| {
                    anyhow!(
                        "Anime {} season[{}] missing 'planned_ep_count'",
                        anime_id,
                        s_i
                    )
                })? as u32,
                eps,
            });
        }

        Ok(AnimeProps {
            data: AnimeBaseData {
                id: anime_id,
                metadata: AnimeMetadata {
                    series_metadata,
                    external_link,
                    titles,
                    air_weekday,
                    air_date,
                    air_quarter,
                    season,
                },
                lock,
            },
        })
    }
}

impl AnimeSqliteClient {
    pub(super) fn build_full_anime_query(
        &self,
        query: &AnimeListQuery,
    ) -> QueryBuilder<sqlx::Sqlite> {
        let mut qb = sqlx::QueryBuilder::new(
            "WITH base AS ( SELECT a.id, a.air_weekday, a.air_date, a.air_quarter, a.is_locked ",
        );

        let mut has_keyword = false;
        let mut fts_str = String::new();
        let mut seq_chars = String::new();

        if let Some(kw) = &query.keyword {
            let tokens = AnimeTitle::to_keywords(kw);
            if !tokens.is_empty() {
                has_keyword = true;
                fts_str = tokens.join(" ");
                seq_chars = tokens.iter().flat_map(|s| s.chars()).collect();
            }
        }

        if has_keyword {
            qb.push(", fts_agg.rank AS rank ");
        } else {
            qb.push(", 0 AS rank ");
        }

        qb.push(" FROM anime a ");

        if has_keyword {
            qb.push(
                " JOIN (SELECT t.anime_id, MIN(fts.rank) as rank 
                FROM anime_title t 
                JOIN anime_alias_fts fts ON t.id = fts.rowid 
                WHERE anime_alias_fts MATCH ",
            );
            qb.push_bind(fts_str);
            qb.push(" GROUP BY t.anime_id) fts_agg ON a.id = fts_agg.anime_id ");
        }

        qb.push(" WHERE 1=1 ");

        if has_keyword {
            let like_pattern: String = format!(
                "%{}%",
                seq_chars
                    .chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("%")
            );
            qb.push(" AND EXISTS (SELECT 1 FROM anime_title t2 WHERE t2.anime_id = a.id AND t2.match_name LIKE ");
            qb.push_bind(like_pattern);
            qb.push(" ESCAPE '\\' ) ");
        }

        if let Some(locked) = query.metadata_locked {
            qb.push(" AND a.is_locked = ");
            qb.push_bind(if locked { 1 } else { 0 });
        }
        if let Some(year) = query.year {
            qb.push(" AND a.air_year = ");
            qb.push_bind(year);
        }
        if let Some(month) = query.month {
            qb.push(" AND a.air_month = ");
            qb.push_bind(month);
        }

        qb.push(" ), ");

        qb.push(" paginated AS ( SELECT * FROM base ");

        if has_keyword {
            qb.push(" ORDER BY rank ASC ");
        } else {
            qb.push(" ORDER BY air_date DESC, id DESC ");
        }

        qb.push(" ), ");

        qb.push(
        r#"
            t_agg AS (
                SELECT anime_id, json_group_array(json_object('name', name, 'match_name', match_name, 'lang_target', lang_target, 'is_origin', is_origin)) AS titles
                FROM (SELECT * FROM anime_title WHERE anime_id IN (SELECT id FROM paginated) ORDER BY anime_id ASC, id ASC)
                GROUP BY anime_id
            ),
            e_agg AS (
                SELECT anime_id, json_group_array(json_object('target_source', target_source, 'ext_type', ext_type, 'ext_id', ext_id)) AS external_links
                FROM (SELECT * FROM anime_external WHERE anime_id IN (SELECT id FROM paginated) ORDER BY anime_id ASC, id ASC)
                GROUP BY anime_id
            ),
            ep_agg AS (
                SELECT season_id, json_group_array(json_object(
                    'ep_number', ep_number, 'sort_number', sort_number, 'air_date', air_date,
                    'duration_seconds', duration_seconds, 'description', description, 'ext_id', ext_id,
                    'titles', json(titles)
                )) AS eps
                FROM (SELECT * FROM anime_episode WHERE anime_id IN (SELECT id FROM paginated) ORDER BY season_id ASC, sort_number ASC)
                GROUP BY season_id
            ),
            s_agg AS (
                SELECT s.anime_id, json_group_array(json_object(
                    'target_source', s.target_source, 'lang_target', s.lang_target,
                    'season_number', s.season_number, 'planned_ep_count', s.planned_ep_count,
                    'description', s.description,
                    'eps', json(COALESCE(ep.eps, '[]'))
                )) AS seasons
                FROM (SELECT * FROM anime_season WHERE anime_id IN (SELECT id FROM paginated) ORDER BY anime_id ASC, season_number ASC, id ASC) s
                LEFT JOIN ep_agg ep ON s.id = ep.season_id
                GROUP BY s.anime_id
            )
            SELECT 
                p.id AS anime_id,
                p.air_weekday,
                p.air_date,
                p.air_quarter,
                p.is_locked,
                s_meta.origin_name AS series_origin_name,
                s_meta.cn_name AS series_cn_name,
                s_meta.description AS series_desc,
                s_meta.air_date AS series_air_date,
                s_meta.genres AS series_genres,
                COALESCE(t.titles, '[]') AS titles,
                COALESCE(e.external_links, '[]') AS external_links,
                COALESCE(s.seasons, '[]') AS seasons
            FROM paginated p
            LEFT JOIN anime_external s_ext ON s_ext.id = (
                SELECT MAX(x.id) FROM anime_external x
                WHERE x.anime_id = p.id AND x.target_source = 'TMDB'
            )
            LEFT JOIN anime_series s_meta ON s_meta.tmdb_id = CAST(s_ext.ext_id AS INTEGER)
            LEFT JOIN t_agg t ON p.id = t.anime_id
            LEFT JOIN e_agg e ON p.id = e.anime_id
            LEFT JOIN s_agg s ON p.id = s.anime_id
            "#
        );

        qb
    }

    // 构建拉平并聚合番剧详细属性的查询
    // filter_applier 用于传入额外的where条件
    pub(super) fn build_anime_details_query<F>(filter_applier: F) -> QueryBuilder<sqlx::Sqlite>
    where
        F: FnOnce(&mut QueryBuilder<sqlx::Sqlite>),
    {
        let mut qb = QueryBuilder::new(
            r#"
    WITH target_animes AS MATERIALIZED (
        SELECT a.id AS anime_id, a.air_weekday, a.air_date, a.air_quarter, a.is_locked
        FROM anime a
    "#,
        );

        filter_applier(&mut qb);

        qb.push(
        r#"
    ),
    t_agg AS (
        SELECT anime_id, json_group_array(json_object('name', name, 'match_name', match_name, 'lang_target', lang_target, 'is_origin', is_origin)) AS titles
        FROM (SELECT * FROM anime_title WHERE anime_id IN (SELECT anime_id FROM target_animes) 
              ORDER BY anime_id ASC, id ASC)
        GROUP BY anime_id
    ),
    e_agg AS (
        SELECT anime_id, json_group_array(json_object('target_source', target_source, 'ext_type', ext_type, 'ext_id', ext_id)) AS external_links
        FROM (SELECT * FROM anime_external WHERE anime_id IN (SELECT anime_id FROM target_animes) 
              ORDER BY anime_id ASC, id ASC)
        GROUP BY anime_id
    ),
    ep_agg AS (
        SELECT season_id, json_group_array(json_object(
            'ep_number', ep_number, 'sort_number', sort_number, 'air_date', air_date,
            'duration_seconds', duration_seconds, 'description', description, 'ext_id', ext_id,
            'titles', json(titles)
        )) AS eps
        FROM (SELECT * FROM anime_episode WHERE anime_id IN (SELECT anime_id FROM target_animes) 
              ORDER BY season_id ASC, sort_number ASC)
        GROUP BY season_id
    ),
    s_agg AS (
        SELECT s.anime_id, json_group_array(json_object(
            'target_source', s.target_source, 'lang_target', s.lang_target,
            'season_number', s.season_number, 'planned_ep_count', s.planned_ep_count, 'description', s.description,
            'eps', json(COALESCE(ep.eps, '[]'))
        )) AS seasons
        FROM (SELECT * FROM anime_season WHERE anime_id IN (SELECT anime_id FROM target_animes) 
              ORDER BY anime_id ASC, season_number ASC, id ASC)
        s LEFT JOIN ep_agg ep ON s.id = ep.season_id
        GROUP BY s.anime_id
    )
    SELECT 
        ta.*,
        s_meta.origin_name AS series_origin_name,
        s_meta.cn_name AS series_cn_name,
        s_meta.description AS series_desc,
        s_meta.air_date AS series_air_date,
        s_meta.genres AS series_genres,
        COALESCE(t.titles, '[]') AS titles,
        COALESCE(e.external_links, '[]') AS external_links,
        COALESCE(s.seasons, '[]') AS seasons
    FROM target_animes ta
    LEFT JOIN anime_external s_ext ON s_ext.id = (
        SELECT MAX(x.id) FROM anime_external x
        WHERE x.anime_id = ta.anime_id AND x.target_source = 'TMDB'
    )
    LEFT JOIN anime_series s_meta ON s_meta.tmdb_id = CAST(s_ext.ext_id AS INTEGER)
    LEFT JOIN t_agg t ON ta.anime_id = t.anime_id
    LEFT JOIN e_agg e ON ta.anime_id = e.anime_id
    LEFT JOIN s_agg s ON ta.anime_id = s.anime_id
    "#
    );
        qb
    }
}

impl AnimeSqliteClient {
    pub(super) async fn update_anime(
        &self,
        anime_id: i64,
        metadata: &AnimeMetadata,
        is_locked: bool,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let mut tmdb_id: i64 = 0;
        for ext in &metadata.external_link {
            if ext.target == AnimeSourceTarget::TMDB {
                tmdb_id = match &ext.id {
                    AnimeIdType::Int(v) => *v,
                    AnimeIdType::String(s) => s
                        .parse()
                        .with_context(|| format!("invalid tmdb id format: {}", s))?,
                };
                break;
            }
        }

        // 系列信息随番剧一起保存：有 TMDB 系列身份、且这次带回了系列信息才写
        if tmdb_id > 0
            && let Some(series_metadata) = &metadata.series_metadata
        {
            let genres_json = serde_json::to_string(&series_metadata.genres)?;
            let series_air_date_str = series_metadata.air_date.format("%Y-%m-%d").to_string();

            sqlx::query(
                "INSERT INTO anime_series (origin_name, cn_name, air_date, description, genres, tmdb_id)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(tmdb_id) DO UPDATE SET
                    origin_name = excluded.origin_name,
                    cn_name = excluded.cn_name,
                    air_date = excluded.air_date,
                    description = excluded.description,
                    genres = excluded.genres,
                    updated_at = unixepoch()",
            )
            .bind(&series_metadata.origin_name)
            .bind(&series_metadata.cn_name)
            .bind(&series_air_date_str)
            .bind(&series_metadata.desc)
            .bind(&genres_json)
            .bind(tmdb_id)
            .execute(&mut *tx)
            .await?;
        }

        let air_year = (metadata.air_quarter / 100) as i32;
        let air_month = (metadata.air_quarter % 100) as i32;

        // 1. 更新主表
        sqlx::query(
            "UPDATE anime SET
            air_weekday = ?,
            air_date = ?,
            air_quarter = ?,
            air_year = ?,
            air_month = ?,
            is_locked = ?,
            updated_at = unixepoch()
         WHERE id = ?",
        )
        // 放送星期允许为空，没写就存 0
        .bind(metadata.air_weekday.clone().map_or(0, i64::from))
        .bind(metadata.air_date.format("%Y-%m-%d").to_string())
        .bind(metadata.air_quarter)
        .bind(air_year)
        .bind(air_month)
        .bind(if is_locked { 1 } else { 0 })
        .bind(anime_id)
        .execute(&mut *tx)
        .await?;

        // 2. 清理旧关联数据（先删 FTS5，再删普通表）
        // 2.1 删除 FTS5 索引（基于旧 title id）
        sqlx::query(
            "DELETE FROM anime_alias_fts
         WHERE rowid IN (SELECT id FROM anime_title WHERE anime_id = ?)",
        )
        .bind(anime_id)
        .execute(&mut *tx)
        .await?;

        // 2.2 删除旧标题、外部链接、剧集、季度
        // 注意顺序：先删剧集（依赖 season_id），再删季度，其他无依赖可随意
        sqlx::query("DELETE FROM anime_episode WHERE anime_id = ?")
            .bind(anime_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM anime_season WHERE anime_id = ?")
            .bind(anime_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM anime_title WHERE anime_id = ?")
            .bind(anime_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM anime_external WHERE anime_id = ?")
            .bind(anime_id)
            .execute(&mut *tx)
            .await?;

        // 3. 插入新数据（复用 insert 逻辑，但注意获取自增 ID 并建立 FTS5）
        // 3.1 插入标题（同时建立 FTS5）
        for title in &metadata.titles {
            let lang_str: String = title.target.clone().into();
            let title_id = sqlx::query(
                "INSERT INTO anime_title (anime_id, name, match_name, lang_target, is_origin)
             VALUES (?, ?, ?, ?, ?)",
            )
            .bind(anime_id)
            .bind(&title.name)
            .bind(&title.match_name)
            .bind(&lang_str)
            .bind(if title.origin { 1 } else { 0 })
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();

            // 插入 FTS5（使用新 title 的 rowid）
            let fts_text = title.keywords().join(" ");
            sqlx::query("INSERT INTO anime_alias_fts (rowid, char_text) VALUES (?, ?)")
                .bind(title_id)
                .bind(&fts_text)
                .execute(&mut *tx)
                .await?;
        }

        // 3.2 插入外部链接
        for ext in &metadata.external_link {
            let target_str: String = ext.target.clone().into();
            let ext_id_str = match &ext.id {
                AnimeIdType::Int(v) => v.to_string(),
                AnimeIdType::String(s) => s.clone(),
            };
            sqlx::query(
                "INSERT INTO anime_external (anime_id, target_source, ext_type, ext_id)
             VALUES (?, ?, ?, ?)",
            )
            .bind(anime_id)
            .bind(target_str)
            .bind(&ext.r#type)
            .bind(ext_id_str)
            .execute(&mut *tx)
            .await?;
        }

        // 3.3 插入季度和剧集
        for season in &metadata.season {
            let target_str: String = season.target.clone().into();
            let lang_str: String = season.lang.clone().into();
            let season_id = sqlx::query(
                "INSERT INTO anime_season
                (anime_id, target_source, lang_target, season_number,
                 planned_ep_count, description)
             VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(anime_id)
            .bind(target_str)
            .bind(&lang_str)
            .bind(season.season)
            .bind(season.planned_episode_count)
            .bind(&season.desc)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();

            for ep in &season.eps {
                let ep_date_str = ep.air_date.format("%Y-%m-%d").to_string();
                let ext_id_str = match &ep.ex_id {
                    AnimeIdType::Int(v) => v.to_string(),
                    AnimeIdType::String(s) => s.clone(),
                };
                let titles_json = serde_json::to_string(&ep.title)?;
                sqlx::query(
                    "INSERT INTO anime_episode (
                    anime_id, season_id, titles, ep_number, sort_number,
                    air_date, duration_seconds, description, ext_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(anime_id)
                .bind(season_id)
                .bind(titles_json)
                .bind(ep.ep)
                .bind(ep.sort)
                .bind(&ep_date_str)
                .bind(ep.duration_seconds as i64)
                .bind(&ep.desc)
                .bind(ext_id_str)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }
}

impl FromContext for AnimeSqliteClient {
    type Ctx = AppContext;

    fn build_from(ctx: &Self::Ctx) -> Result<Self> {
        Ok(Self::new(ctx.pool.clone()))
    }
}
