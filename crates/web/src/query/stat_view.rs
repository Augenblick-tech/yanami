use anyhow::Result;
use sqlx::{Pool, Sqlite};

#[derive(Clone)]
pub struct StatQuery {
    pub pool: Pool<Sqlite>,
}

impl StatQuery {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn get_system_stat(
        &self,
        space_id: i64,
        backoff_ids: &[(i64, i64)],
    ) -> Result<crate::model::SystemStatResponse> {
        // 1. 获取总番剧数
        let total_anime_row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM anime")
            .fetch_one(&self.pool)
            .await?;
        let total_anime_count = total_anime_row.0;

        // 2. 获取当前用户订阅的番剧数
        let sub_anime_row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sub_anime WHERE space_id = ?")
                .bind(space_id)
                .fetch_one(&self.pool)
                .await?;
        let user_subscribed_count = sub_anime_row.0;

        // 3. 获取等待搜索的委托数
        let mandate_row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM search_mandate")
            .fetch_one(&self.pool)
            .await?;
        let waiting_mandates_count = mandate_row.0;

        // 4. 获取各季度统计
        let quarter_rows = sqlx::query(
            "
            SELECT 
                a.air_quarter,
                COUNT(1) AS total_count,
                SUM(CASE WHEN sa.id IS NOT NULL THEN 1 ELSE 0 END) AS sub_count,
                SUM(CASE WHEN sa.id IS NOT NULL AND sa.progress = 0 THEN 1 ELSE 0 END) AS not_started_count,
                SUM(CASE WHEN sa.id IS NOT NULL AND sa.progress > 0 AND sa.progress < COALESCE((SELECT planned_ep_count FROM anime_season s WHERE s.anime_id = a.id ORDER BY season_number ASC LIMIT 1), 9999) THEN 1 ELSE 0 END) AS updating_count,
                SUM(CASE WHEN sa.progress >= COALESCE((SELECT planned_ep_count FROM anime_season s WHERE s.anime_id = a.id ORDER BY season_number ASC LIMIT 1), 9999) THEN 1 ELSE 0 END) AS completed_count,
                SUM(CASE WHEN sa.search_status = 0 THEN 1 ELSE 0 END) AS not_search_count,
                SUM(CASE WHEN sa.search_status = 1 THEN 1 ELSE 0 END) AS pending_count,
                SUM(CASE WHEN sa.search_status = 2 THEN 1 ELSE 0 END) AS matching_count,
                SUM(CASE WHEN sa.search_status = 3 THEN 1 ELSE 0 END) AS searching_count
            FROM anime a
            LEFT JOIN sub_anime sa ON a.id = sa.anime_id AND sa.space_id = ?
            GROUP BY a.air_quarter
            ORDER BY a.air_quarter DESC
            "
        )
        .bind(space_id)
        .fetch_all(&self.pool)
        .await?;

        let mut quarter_stats = Vec::new();
        use sqlx::Row;
        for row in quarter_rows {
            quarter_stats.push(crate::model::QuarterStat {
                quarter: row.get::<i32, _>("air_quarter") as u32,
                total_count: row.get::<i64, _>("total_count"),
                sub_count: row.get::<i64, _>("sub_count"),
                not_started_count: row.get::<i64, _>("not_started_count"),
                updating_count: row.get::<i64, _>("updating_count"),
                completed_count: row.get::<i64, _>("completed_count"),
                not_search_count: row.get::<i64, _>("not_search_count"),
                pending_count: row.get::<i64, _>("pending_count"),
                matching_count: row.get::<i64, _>("matching_count"),
                searching_count: row.get::<i64, _>("searching_count"),
            });
        }

        let mut backoff_feeds = Vec::new();
        if !backoff_ids.is_empty() {
            let mut qb = sqlx::QueryBuilder::new("SELECT id, title FROM feed WHERE id IN (");
            let mut separated = qb.separated(", ");
            for (id, _) in backoff_ids.iter() {
                separated.push_bind(*id);
            }
            separated.push_unseparated(")");
            let rows = qb.build().fetch_all(&self.pool).await?;
            let mut name_map = std::collections::HashMap::new();
            for row in rows {
                let id: i64 = row.get("id");
                let title: String = row.get("title");
                name_map.insert(id, title);
            }
            for (id, ts) in backoff_ids {
                if let Some(title) = name_map.get(id) {
                    backoff_feeds.push(crate::model::BackoffFeed {
                        feed_id: *id,
                        feed_name: title.clone(),
                        backoff_until: *ts,
                    });
                }
            }
        }

        Ok(crate::model::SystemStatResponse {
            total_anime_count,
            user_subscribed_count,
            waiting_mandates_count,
            backoff_feeds,
            quarter_stats,
        })
    }
}
