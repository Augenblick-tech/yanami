use std::{collections::HashMap, ops::ControlFlow};

use crate::{
    entity::{
        cap::{AnimeConsumer, AnimeRepository},
        model::{
            AnimeBaseData, AnimeIdType, AnimeListQuery, AnimeMetadata, AnimeProps,
            AnimeSeriesMetadata, AnimeSourceTarget,
        },
    },
    infra::repository::client::AnimeSqliteClient,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::NaiveDate;
use futures::StreamExt;
use sqlx::{QueryBuilder, Row};

#[async_trait]
impl AnimeRepository for AnimeSqliteClient {
    async fn list(&self, query: &AnimeListQuery) -> Result<Vec<AnimeProps>> {
        let mut qb = self.build_full_anime_query(query);
        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .context("failed to execute unified anime query")?;

        let mut data = Vec::with_capacity(rows.len());

        for row in rows {
            let props = Self::parse_anime_row(&row)?;
            data.push(props);
        }

        Ok(data)
    }

    async fn range(&self, query: &AnimeListQuery, consumer: &mut dyn AnimeConsumer) -> Result<()> {
        let mut qb = self.build_full_anime_query(query);
        let mut stream = qb.build().fetch(&self.pool);

        while let Some(row_result) = stream.next().await {
            let row = row_result.context("failed to fetch row from stream in range")?;
            let props = Self::parse_anime_row(&row)?;
            match consumer.consume(props)? {
                ControlFlow::Continue(_) => continue,
                ControlFlow::Break(_) => break,
            }
        }

        Ok(())
    }

    async fn find(&self, anime_id: i64) -> Result<Option<AnimeProps>> {
        let mut qb = Self::build_anime_details_query(|qb| {
            qb.push(" WHERE a.id = ");
            qb.push_bind(anime_id);
        });

        let row_opt = qb.build().fetch_optional(&self.pool).await?;

        match row_opt {
            Some(row) => Ok(Some(Self::parse_anime_row(&row)?)),
            None => Ok(None),
        }
    }

    async fn list_by_ids(&self, anime_ids: &[i64]) -> Result<Vec<AnimeProps>> {
        if anime_ids.is_empty() {
            return Ok(Vec::new());
        }

        let ids_json = serde_json::to_string(anime_ids)
            .context("Failed to serialize anime_ids to JSON string")?;

        let mut qb = Self::build_anime_details_query(|qb| {
            qb.push(" WHERE a.id IN (SELECT value FROM json_each(");
            qb.push_bind(ids_json);
            qb.push("))");
        });

        let mut results = Vec::with_capacity(anime_ids.len());
        let mut stream = qb.build().fetch(&self.pool);

        while let Some(row_result) = stream.next().await {
            let row = row_result.context("Failed to fetch row from stream in list_by_ids")?;
            results.push(Self::parse_anime_row(&row)?);
        }

        Ok(results)
    }

    async fn insert(&self, entity: &AnimeMetadata, lock: bool) -> Result<AnimeProps> {
        let mut tx = self.pool.begin().await?;

        let mut tmdb_id: i64 = 0;
        for ext in &entity.external_link {
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
            && let Some(series_metadata) = &entity.series_metadata
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

        let air_year = (entity.air_quarter / 100) as i32;
        let air_month = (entity.air_quarter % 100) as i32;
        let weekday_i64: i64 = entity.air_weekday.clone().into();
        let air_date_str = entity.air_date.format("%Y-%m-%d").to_string();

        let anime_id = sqlx::query(
            "INSERT INTO anime (air_weekday, air_date, air_quarter, air_year, air_month, is_locked) 
            VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(weekday_i64)
        .bind(&air_date_str)
        .bind(entity.air_quarter)
        .bind(air_year)
        .bind(air_month)
        .bind(if lock { 1 } else { 0 })
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();

        for title in &entity.titles {
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

            // 插入FTS5索引
            let fts_char_text = title.keywords().join(" ");
            sqlx::query("INSERT INTO anime_alias_fts (rowid, char_text) VALUES (?, ?)")
                .bind(title_id)
                .bind(&fts_char_text)
                .execute(&mut *tx)
                .await?;
        }

        for ext in &entity.external_link {
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

        for season in &entity.season {
            let target_str: String = season.target.clone().into();
            let lang_str: String = season.lang.clone().into();

            let season_id = sqlx::query(
                "INSERT INTO anime_season (anime_id, target_source, lang_target, season_number, planned_ep_count, description) 
                VALUES (?, ?, ?, ?, ?, ?)"
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

        Ok(AnimeProps {
            data: AnimeBaseData {
                id: anime_id,
                metadata: entity.clone(),
                lock,
            },
        })
    }

    async fn update(&self, entity: &AnimeBaseData) -> Result<()> {
        self.update_anime(entity.id, &entity.metadata, entity.lock)
            .await
    }

    async fn set_lock(&self, anime_id: i64, lock: bool) -> Result<()> {
        let rows_affected =
            sqlx::query("UPDATE anime SET is_locked = ?, updated_at = unixepoch() WHERE id = ?")
                .bind(if lock { 1 } else { 0 })
                .bind(anime_id)
                .execute(&self.pool)
                .await?
                .rows_affected();

        if rows_affected == 0 {
            return Err(anyhow!("Anime with id {} not found", anime_id));
        }
        Ok(())
    }

    async fn sync_metadata_with_not_lock(
        &self,
        metadata: Vec<AnimeMetadata>,
    ) -> Result<Vec<AnimeProps>> {
        let mut bgm_to_meta = HashMap::new();
        let mut bgm_ids = Vec::new();
        for meta in &metadata {
            let bgm_ext = meta
                .external_link
                .iter()
                .find(|e| matches!(e.target, AnimeSourceTarget::Bangumi))
                .context("Missing Bangumi external link")?;
            let bgm_id = match &bgm_ext.id {
                AnimeIdType::Int(v) => *v,
                AnimeIdType::String(s) => s.parse::<i64>()?,
            };
            bgm_ids.push(bgm_id);
            bgm_to_meta.insert(bgm_id, meta);
        }

        let mut bgm_to_anime_id = HashMap::new();
        let mut existing_anime_ids = Vec::new();
        if !bgm_ids.is_empty() {
            let mut qb = QueryBuilder::new(
                "SELECT ae.ext_id, ae.anime_id
             FROM anime_external ae
             JOIN anime a ON ae.anime_id = a.id
             WHERE ae.target_source = 'Bangumi' AND ae.ext_id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in &bgm_ids {
                sep.push_bind(id.to_string());
            }
            sep.push_unseparated(")");
            let rows = qb.build().fetch_all(&self.pool).await?;
            for row in rows {
                let ext_id: String = row.try_get("ext_id")?;
                let anime_id: i64 = row.try_get("anime_id")?;
                let bgm_id = ext_id.parse::<i64>()?;
                bgm_to_anime_id.insert(bgm_id, anime_id);
                existing_anime_ids.push(anime_id);
            }
        }

        let existing_props_list = self.list_by_ids(&existing_anime_ids).await?;
        let existing_props_map: HashMap<i64, AnimeProps> = existing_props_list
            .into_iter()
            .map(|p| (p.data.id, p))
            .collect();

        let mut props = vec![];
        for (bgm_id, meta) in bgm_to_meta {
            if let Some(&anime_id) = bgm_to_anime_id.get(&bgm_id) {
                if let Some(existing_prop) = existing_props_map.get(&anime_id) {
                    if existing_prop.data.lock {
                        continue;
                    }
                    if meta == &existing_prop.data.metadata {
                        props.push(existing_prop.clone());
                        continue;
                    }
                }

                if let Err(e) = self.update_anime(anime_id, meta, false).await {
                    tracing::error!("sync_metadata_with_not_lock update metadata failed, {}", e);
                } else if let Ok(Some(updated_prop)) = self.find(anime_id).await {
                    props.push(updated_prop);
                }
            } else {
                match self.insert(meta, false).await {
                    Ok(prop) => {
                        props.push(prop);
                    }
                    Err(e) => {
                        tracing::error!(
                            "sync_metadata_with_not_lock insert metadata failed, {}",
                            e
                        );
                    }
                }
            }
        }
        Ok(props)
    }

    async fn find_series(&self, tmdb_id: i64) -> Result<Option<AnimeSeriesMetadata>> {
        let series_row = sqlx::query(
            "SELECT origin_name, cn_name, description, air_date, genres
             FROM anime_series WHERE tmdb_id = ?",
        )
        .bind(tmdb_id)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to query anime series by tmdb id {}", tmdb_id))?;

        let Some(series_row) = series_row else {
            return Ok(None);
        };

        let air_date_str: String = series_row
            .try_get("air_date")
            .with_context(|| format!("Anime series {} missing 'air_date'", tmdb_id))?;
        let air_date = NaiveDate::parse_from_str(&air_date_str, "%Y-%m-%d").with_context(|| {
            format!(
                "Anime series {} has invalid air_date format: {}",
                tmdb_id, air_date_str
            )
        })?;

        let genres_json: String = series_row
            .try_get("genres")
            .with_context(|| format!("Anime series {} missing 'genres'", tmdb_id))?;
        let genres: Vec<String> = serde_json::from_str(&genres_json)
            .with_context(|| format!("Anime series {} genres JSON parse failed", tmdb_id))?;

        Ok(Some(AnimeSeriesMetadata {
            origin_name: series_row
                .try_get("origin_name")
                .with_context(|| format!("Anime series {} missing 'origin_name'", tmdb_id))?,
            cn_name: series_row
                .try_get("cn_name")
                .with_context(|| format!("Anime series {} missing 'cn_name'", tmdb_id))?,
            desc: series_row
                .try_get("description")
                .with_context(|| format!("Anime series {} missing 'description'", tmdb_id))?,
            air_date,
            genres,
        }))
    }

    async fn list_by_series(&self, tmdb_id: i64) -> Result<Vec<AnimeProps>> {
        // 归属只看番剧自己的 TMDB 外部身份，与 list/find 的联表口径一致
        let mut qb = Self::build_anime_details_query(|qb| {
            qb.push(
                " WHERE EXISTS (SELECT 1 FROM anime_external x
                    WHERE x.anime_id = a.id AND x.target_source = 'TMDB'
                      AND CAST(x.ext_id AS INTEGER) = ",
            );
            qb.push_bind(tmdb_id);
            qb.push(")");
        });

        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .with_context(|| format!("failed to query animes of series {}", tmdb_id))?;

        let mut animes = Vec::with_capacity(rows.len());
        for row in rows {
            animes.push(Self::parse_anime_row(&row)?);
        }

        Ok(animes)
    }
}

impl AnimeSqliteClient {
    /// 取回还没有系列展示信息的 TMDB 剧集 id：归属只看番剧自己的 TMDB 外部身份，与 list_by_series 口径一致。
    pub async fn list_missing_series_ids(&self) -> Result<Vec<i64>> {
        let rows = sqlx::query(
            "SELECT DISTINCT CAST(x.ext_id AS INTEGER) AS tmdb_id
             FROM anime_external x
             WHERE x.target_source = 'TMDB'
               AND CAST(x.ext_id AS INTEGER) > 0
               AND NOT EXISTS (
                   SELECT 1 FROM anime_series s WHERE s.tmdb_id = CAST(x.ext_id AS INTEGER)
               )",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to query tmdb ids missing series")?;

        let mut tmdb_ids = Vec::with_capacity(rows.len());
        for row in rows {
            tmdb_ids.push(row.try_get("tmdb_id")?);
        }

        Ok(tmdb_ids)
    }

    /// 一次性写入系列展示信息，要么全写要么全不写。
    pub async fn save_series(&self, series: &[(i64, AnimeSeriesMetadata)]) -> Result<()> {
        if series.is_empty() {
            return Ok(());
        }

        let mut genres = Vec::with_capacity(series.len());
        for (_, metadata) in series {
            genres.push(serde_json::to_string(&metadata.genres)?);
        }

        let mut tx = self.pool.begin().await?;

        let mut qb = QueryBuilder::new(
            "INSERT INTO anime_series (origin_name, cn_name, air_date, description, genres, tmdb_id) ",
        );
        qb.push_values(
            series.iter().zip(&genres),
            |mut b, ((tmdb_id, metadata), genres)| {
                b.push_bind(&metadata.origin_name)
                    .push_bind(&metadata.cn_name)
                    .push_bind(metadata.air_date.format("%Y-%m-%d").to_string())
                    .push_bind(&metadata.desc)
                    .push_bind(genres)
                    .push_bind(*tmdb_id);
            },
        );
        qb.push(
            " ON CONFLICT(tmdb_id) DO UPDATE SET
                origin_name = excluded.origin_name,
                cn_name = excluded.cn_name,
                air_date = excluded.air_date,
                description = excluded.description,
                genres = excluded.genres,
                updated_at = unixepoch()",
        );

        qb.build()
            .execute(&mut *tx)
            .await
            .context("failed to save anime series")?;

        tx.commit().await?;

        Ok(())
    }
}
