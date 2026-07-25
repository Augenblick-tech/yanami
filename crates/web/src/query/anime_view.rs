use anyhow::Result;
use sqlx::{Pool, Row, Sqlite};

use crate::model::{AnimeResponse, AnimeSubInfo, Page, PageAnimeRequest, RecentEpisodeResponse};
use anime::entity::model::AnimeLangTarget;

#[derive(Clone)]
pub struct AnimeViewQuery {
    pub pool: Pool<Sqlite>,
}

impl AnimeViewQuery {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn page_anime_views(
        &self,
        req: &PageAnimeRequest,
        space_id: i64,
        page: usize,
        page_size: usize,
    ) -> Result<Page<Vec<AnimeResponse>>> {
        let mut qb =
            sqlx::QueryBuilder::new("WITH base AS ( SELECT a.id, a.air_weekday, a.air_date ");

        let mut has_keyword = false;
        let mut fts_str = String::new();
        let mut seq_chars = String::new();

        if let Some(kw) = &req.keyword {
            let tokens = anime::entity::model::AnimeTitle::to_keywords(kw);
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

        qb.push(" LEFT JOIN sub_anime sa ON sa.anime_id = a.id AND sa.space_id = ");
        qb.push_bind(space_id);

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
            qb.push(
                " AND EXISTS (SELECT 1 FROM anime_title t2 WHERE t2.anime_id = a.id AND t2.match_name LIKE ",
            );
            qb.push_bind(like_pattern);
            qb.push(" ESCAPE '\\' ) ");
        }

        if let Some(year) = req.year {
            qb.push(" AND a.air_year = ");
            qb.push_bind(year);
        }

        if let Some(month) = req.month {
            qb.push(" AND a.air_month = ");
            qb.push_bind(month);
        }

        if let Some(is_sub) = req.subscription {
            if is_sub {
                qb.push(" AND sa.space_id IS NOT NULL ");
            } else {
                qb.push(" AND sa.space_id IS NULL ");
            }
        }

        if let Some(st) = req.status {
            match st {
                1 => {
                    qb.push(" AND sa.space_id IS NOT NULL ");
                }
                2 => {
                    qb.push(" AND sa.space_id IS NOT NULL AND sa.progress >= COALESCE((SELECT planned_ep_count FROM anime_season s WHERE s.anime_id = a.id ORDER BY CASE WHEN s.target_source = 'Bangumi' THEN 0 ELSE 1 END ASC, season_number ASC LIMIT 1), 9999) ");
                }
                3 => {
                    qb.push(" AND sa.space_id IS NOT NULL AND sa.progress = 0 ");
                }
                4 => {
                    qb.push(" AND sa.space_id IS NOT NULL AND sa.progress > 0 AND sa.progress < COALESCE((SELECT planned_ep_count FROM anime_season s WHERE s.anime_id = a.id ORDER BY CASE WHEN s.target_source = 'Bangumi' THEN 0 ELSE 1 END ASC, season_number ASC LIMIT 1), 9999) ");
                }
                _ => {}
            }
        }

        if let Some(search_status) = req.search_status {
            qb.push(" AND sa.search_status = ");
            qb.push_bind(search_status);
        }

        qb.push(" ), paginated AS ( SELECT *, COUNT(*) OVER() AS total_count FROM base ");

        if has_keyword {
            qb.push(" ORDER BY rank ASC ");
        } else {
            qb.push(" ORDER BY air_date DESC, id DESC ");
        }

        let offset = (page - 1) * page_size;

        qb.push(" LIMIT ");
        qb.push_bind(page_size as i64);
        qb.push(" OFFSET ");
        qb.push_bind(offset as i64);
        qb.push(" ) ");

        qb.push(
            "SELECT 
                p.id AS anime_id,
                p.air_weekday,
                p.air_date,
                p.total_count,
                sa.id AS sub_anime_id,
                sa.search_status,
                sa.progress,
                (SELECT name FROM anime_title t WHERE t.anime_id = p.id AND t.is_origin = 1 LIMIT 1) AS origin_name,
                ",
        );

        if let Some(lang) = &req.lang {
            let target_lang_db: String = AnimeLangTarget::from(lang.as_str()).into();
            qb.push("(SELECT name FROM anime_title t WHERE t.anime_id = p.id AND t.lang_target = ");
            qb.push_bind(target_lang_db);
            qb.push(" LIMIT 1) AS lang_name, ");
        } else {
            qb.push("NULL AS lang_name, ");
        }

        qb.push(
            "
                (SELECT planned_ep_count FROM anime_season s WHERE s.anime_id = p.id ORDER BY CASE WHEN s.target_source = 'Bangumi' THEN 0 ELSE 1 END ASC, season_number ASC LIMIT 1) AS eps,
                (SELECT season_number FROM anime_season s WHERE s.anime_id = p.id ORDER BY CASE WHEN s.target_source = 'Bangumi' THEN 0 ELSE 1 END ASC, season_number ASC LIMIT 1) AS season,
                (SELECT description FROM anime_season s WHERE s.anime_id = p.id ORDER BY CASE WHEN s.target_source = 'Bangumi' THEN 0 ELSE 1 END ASC, season_number ASC LIMIT 1) AS desc
            FROM paginated p
            LEFT JOIN sub_anime sa ON sa.anime_id = p.id AND sa.space_id = "
        );
        qb.push_bind(space_id);

        let query = qb.build();
        let rows = query.fetch_all(&self.pool).await?;

        let mut data = Vec::new();
        let mut total = 0;

        for row in rows {
            if total == 0 {
                total = row.get::<i64, _>("total_count") as u64;
            }

            let anime_id = row.get::<i64, _>("anime_id");
            let origin_name = row.get::<String, _>("origin_name");

            let lang_name: Option<String> = row.try_get("lang_name").unwrap_or(None);

            let desc = row
                .get::<Option<String>, _>("desc")
                .ok_or_else(|| anyhow::anyhow!("anime {} missing desc", anime_id))?;
            let air_date = row
                .get::<Option<String>, _>("air_date")
                .ok_or_else(|| anyhow::anyhow!("anime {} missing air_date", anime_id))?;
            let air_weekday = row.get::<i64, _>("air_weekday");
            let eps = row
                .get::<Option<i32>, _>("eps")
                .ok_or_else(|| anyhow::anyhow!("anime {} missing eps", anime_id))?
                as u32;
            let season = row
                .get::<Option<i32>, _>("season")
                .ok_or_else(|| anyhow::anyhow!("anime {} missing season", anime_id))?
                as u32;

            let sub_anime_id: Option<i64> = row.get("sub_anime_id");

            let sub_info = sub_anime_id.map(|id| AnimeSubInfo {
                sub_anime_id: id,
                search_status: row.get::<i32, _>("search_status"),
                progress: row.get::<i32, _>("progress") as u32,
            });

            data.push(AnimeResponse {
                id: anime_id,
                name: origin_name,
                name_target: lang_name,
                desc,
                air_date,
                air_weekday,
                eps,
                season,
                sub_info,
            });
        }

        Ok(Page {
            page,
            page_size,
            total,
            data,
        })
    }

    pub async fn recent_episodes(
        &self,
        space_id: i64,
        lang: Option<String>,
    ) -> Result<Vec<RecentEpisodeResponse>> {
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT
                e.ep_num,
                e.updated_at,
                r.name AS rule_name,
                (SELECT t.name FROM anime_title t WHERE t.anime_id = sa.anime_id AND t.is_origin = 1 LIMIT 1) AS origin_name,
                "
        );

        if let Some(l) = lang {
            let target_lang_db: String = AnimeLangTarget::from(l.as_str()).into();
            qb.push("(SELECT t.name FROM anime_title t WHERE t.anime_id = sa.anime_id AND t.lang_target = ");
            qb.push_bind(target_lang_db);
            qb.push(" LIMIT 1) AS lang_name ");
        } else {
            qb.push("NULL AS lang_name ");
        }

        qb.push(
            "FROM sub_anime_episode e
            JOIN sub_anime sa ON sa.id = e.sub_anime_id
            LEFT JOIN rule r ON r.id = sa.rule_id
            WHERE sa.space_id = ",
        );
        qb.push_bind(space_id);
        qb.push(" ORDER BY e.updated_at DESC LIMIT 10");

        let query = qb.build();
        let rows = query.fetch_all(&self.pool).await?;

        let mut data = Vec::with_capacity(rows.len());
        for row in rows {
            let origin_name = row.get::<String, _>("origin_name");
            let lang_name: Option<String> = row.try_get("lang_name").unwrap_or(None);
            let ep_num: Option<f64> = row.try_get("ep_num").unwrap_or(None);
            let rule_name: Option<String> = row.try_get("rule_name").unwrap_or(None);
            let updated_at = row.get::<i64, _>("updated_at");

            data.push(RecentEpisodeResponse {
                origin_name,
                lang_name,
                ep_num,
                rule_name,
                updated_at,
            });
        }

        Ok(data)
    }
}
