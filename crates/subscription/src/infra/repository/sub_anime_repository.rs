use anyhow::{Result, anyhow};
use async_trait::async_trait;
use sqlx::QueryBuilder;

use crate::{
    entity::{
        cap::SubAnimeRepository,
        model::{
            Episode, EpisodeBaseData, EpisodeProp, SubAnimeBaseData, SubAnimeListQuery,
            SubAnimeProps, SubAnimeStatus,
        },
    },
    infra::repository::client::SubAnimeSqliteClient,
};

#[async_trait]
impl SubAnimeRepository for SubAnimeSqliteClient {
    async fn insert_sub_anime(&self, space_id: i64, anime_id: i64) -> Result<SubAnimeProps> {
        let status = i32::from(crate::entity::model::SubAnimeSearchStatus::Pending);
        let insert_result = sqlx::query(
            "INSERT INTO sub_anime (anime_id, space_id, search_status) VALUES (?, ?, ?)",
        )
        .bind(anime_id)
        .bind(space_id)
        .bind(status)
        .execute(&self.pool)
        .await;

        let inserted_id = match insert_result {
            Ok(r) => r.last_insert_rowid(),
            Err(e) => {
                if let sqlx::Error::Database(db_err) = &e
                    && db_err.kind() == sqlx::error::ErrorKind::UniqueViolation
                {
                    return Err(anyhow!(
                        "subscription already exists for space_id {} and anime_id {}",
                        space_id,
                        anime_id
                    ));
                }
                return Err(e.into());
            }
        };

        if inserted_id == 0 {
            return Err(anyhow!("insert into sub_anime returned 0 rows affected"));
        }
        self.find_sub_anime(inserted_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("inserted sub anime not found"))
    }

    async fn delete(&self, sub_anime: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM sub_anime_episode WHERE sub_anime_id = ?")
            .bind(sub_anime)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM sub_anime WHERE id = ?")
            .bind(sub_anime)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn update_sub_anime(&self, data: &SubAnimeBaseData) -> Result<()> {
        self.update_sub_animes(std::slice::from_ref(data)).await
    }

    async fn update_sub_animes(&self, data: &[SubAnimeBaseData]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let mut builder = QueryBuilder::new("UPDATE sub_anime SET ");

        builder.push("rule_id = CASE id");
        for item in data {
            builder
                .push(" WHEN ")
                .push_bind(item.id)
                .push(" THEN ")
                .push_bind(item.rule_id);
        }
        builder.push(" END, ");

        builder.push("search_status = CASE id");
        for item in data {
            let status: i32 = item.search_status.into();
            builder
                .push(" WHEN ")
                .push_bind(item.id)
                .push(" THEN ")
                .push_bind(status);
        }
        builder.push(" END, ");

        builder.push("progress = CASE id");
        for item in data {
            builder
                .push(" WHEN ")
                .push_bind(item.id)
                .push(" THEN ")
                .push_bind(item.progress as i32);
        }
        builder.push(" END WHERE id IN (");

        let mut separated = builder.separated(", ");
        for item in data {
            separated.push_bind(item.id);
        }
        separated.push_unseparated(")");

        builder.build().execute(&self.pool).await?;

        Ok(())
    }

    async fn find_sub_anime(&self, id: i64) -> Result<Option<SubAnimeProps>> {
        let mut builder = QueryBuilder::new(Self::BASE_SELECT_JOIN);
        builder.push(" WHERE sa.id = ");
        builder.push_bind(id);
        builder.push(" GROUP BY sa.id");

        let row = builder.build().fetch_optional(&self.pool).await?;
        row.map(|r| Self::row_to_sub_anime_props(&r)).transpose()
    }

    async fn find_by_anime_ids(
        &self,
        space_id: i64,
        anime_ids: Vec<i64>,
    ) -> Result<Vec<SubAnimeProps>> {
        if anime_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut builder = QueryBuilder::new(Self::BASE_SELECT_JOIN);
        builder.push(" WHERE sa.space_id = ");
        builder.push_bind(space_id);
        builder.push(" AND sa.anime_id IN (");

        let mut separated = builder.separated(", ");
        for id in anime_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        builder.push(" GROUP BY sa.id");

        let rows = builder.build().fetch_all(&self.pool).await?;
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(Self::row_to_sub_anime_props(&row)?);
        }
        Ok(results)
    }

    async fn list(&self, query: &SubAnimeListQuery) -> Result<Vec<SubAnimeProps>> {
        let mut builder = QueryBuilder::new(Self::BASE_SELECT_JOIN);
        let mut has_condition = false;

        if let Some(space_id) = query.space_id {
            builder.push(" WHERE sa.space_id = ");
            builder.push_bind(space_id);
            has_condition = true;
        }
        if let Some(anime_id) = query.anime_id {
            builder.push(if has_condition { " AND " } else { " WHERE " });
            builder.push("sa.anime_id = ");
            builder.push_bind(anime_id);
            has_condition = true;
        }
        if let Some(search_status) = query.search_status {
            builder.push(if has_condition { " AND " } else { " WHERE " });
            builder.push("sa.search_status = ");
            builder.push_bind(i32::from(search_status));
            has_condition = true;
        }
        if let Some(sub_status) = &query.sub_status {
            builder.push(if has_condition { " AND " } else { " WHERE " });
            // eps 是子查询计算出来的，这里直接复用原查询中的 eps 表达式
            // progress >= eps → Completed，否则 Enable
            match sub_status {
                SubAnimeStatus::Completed => {
                    builder.push(
                        "sa.progress >= COALESCE((SELECT planned_ep_count FROM anime_season WHERE anime_id = sa.anime_id AND target_source = 'Bangumi'), 0)"
                    );
                }
                SubAnimeStatus::Enable => {
                    builder.push(
                        "sa.progress < COALESCE((SELECT planned_ep_count FROM anime_season WHERE anime_id = sa.anime_id AND target_source = 'Bangumi'), 0)"
                    );
                }
            }
        }

        builder.push(" GROUP BY sa.id");

        if let Some(limit) = query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(limit as i64);
        }

        let rows = builder.build().fetch_all(&self.pool).await?;
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(Self::row_to_sub_anime_props(&row)?);
        }
        Ok(results)
    }

    async fn list_eps(&self, sub_anime_id: i64) -> Result<Vec<EpisodeProp>> {
        let mut builder = QueryBuilder::new(Self::EPISODE_SELECT_JOIN);
        builder.push(" WHERE se.sub_anime_id = ");
        builder.push_bind(sub_anime_id);
        builder.push(" ORDER BY se.ep_num ASC");

        let rows = builder.build().fetch_all(&self.pool).await?;
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(Self::row_to_episode_prop(&row)?);
        }
        Ok(results)
    }

    async fn find_epsiode(&self, ep_id: i64) -> Result<Option<EpisodeProp>> {
        let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(Self::EPISODE_SELECT_JOIN);
        builder.push(" WHERE se.id = ");
        builder.push_bind(ep_id);

        let row = builder.build().fetch_optional(&self.pool).await?;
        row.map(|r| Self::row_to_episode_prop(&r)).transpose()
    }

    async fn get_one_undownload_ep(&self) -> Result<Option<EpisodeProp>> {
        let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(Self::EPISODE_SELECT_JOIN);

        builder.push(" WHERE se.status = ");
        builder.push_bind(i32::from(crate::entity::model::EpsiodeStatus::Pending));
        builder.push(" ORDER BY RANDOM() LIMIT 1");

        let row = builder.build().fetch_optional(&self.pool).await?;

        row.map(|r| Self::row_to_episode_prop(&r)).transpose()
    }

    async fn update_epsiode_status(&self, data: &EpisodeBaseData) -> Result<()> {
        let sql = "UPDATE sub_anime_episode 
            SET status = ?, updated_at = (unixepoch())
            WHERE id = ?";
        sqlx::query(sql)
            .bind(i32::from(data.ep.status.clone()))
            .bind(data.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_epsiodes_status(&self, data: &[EpisodeBaseData]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let mut builder = QueryBuilder::new("UPDATE sub_anime_episode SET ");

        builder.push("status = CASE id");
        for item in data {
            let status: i32 = item.ep.status.clone().into();
            builder
                .push(" WHEN ")
                .push_bind(item.id)
                .push(" THEN ")
                .push_bind(status);
        }
        builder.push(" END, updated_at = (unixepoch()) WHERE id IN (");

        let mut separated = builder.separated(", ");
        for item in data {
            separated.push_bind(item.id);
        }
        separated.push_unseparated(")");

        builder.build().execute(&self.pool).await?;
        Ok(())
    }

    async fn update_sub_anime_progress(
        &self,
        data: &SubAnimeBaseData,
        eps: &[Episode],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE sub_anime SET progress = ?, rule_id = ? WHERE id = ?")
            .bind(data.progress as i32)
            .bind(data.rule_id)
            .bind(data.id)
            .execute(&mut *tx)
            .await?;

        if !eps.is_empty() {
            let mut builder = QueryBuilder::new(
                "INSERT INTO sub_anime_episode (sub_anime_id, resource_id, status, ep_num) ",
            );
            builder.push_values(eps, |mut b, ep| {
                b.push_bind(ep.sub_anime_id)
                    .push_bind(ep.resource_id.as_slice())
                    .push_bind(i32::from(ep.status.clone()))
                    .push_bind(ep.ep_num);
            });
            builder.push(
                " ON CONFLICT (sub_anime_id, resource_id) DO UPDATE SET ep_num = excluded.ep_num",
            );

            builder.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn binding_rule_and_clear_eps(&self, sub_anime: i64, rule_id: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            "UPDATE sub_anime 
             SET rule_id = ?, progress = 0 
             WHERE id = ? 
               AND space_id = (SELECT space_id FROM rule WHERE id = ?)",
        )
        .bind(rule_id)
        .bind(sub_anime)
        .bind(rule_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(anyhow!(
                "binding failed: sub_anime not found, rule not found, or space_id mismatch"
            ));
        }

        sqlx::query("DELETE FROM sub_anime_episode WHERE sub_anime_id = ?")
            .bind(sub_anime)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
