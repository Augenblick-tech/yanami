use anyhow::{Result, anyhow};
use async_trait::async_trait;
use sqlx::QueryBuilder;

use crate::{
    entity::{
        cap::SubAnimeRepository,
        model::{
            Episode, EpisodeBaseData, EpisodeProp, SubAnimeBaseData, SubAnimeListQuery,
            SubAnimeProps, SubAnimeSearchStatus, SubAnimeStatus,
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

        // 订阅删除时，搜索委托上属于它的记录一起删掉
        sqlx::query("DELETE FROM search_mandate_sub_anime WHERE sub_anime_id = ?")
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
            // 搜索中不存订阅表：订阅交给搜索委托之后，这里落成不搜索
            let status: i32 = match item.search_status {
                SubAnimeSearchStatus::Searching => SubAnimeSearchStatus::NotSearch.into(),
                other => other.into(),
            };
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
            // 搜索中(3)、不搜索(0) 由搜索委托上的记录定；等待中(1)、匹配中(2) 按订阅表里存的状态取。
            // 等待中(1)、匹配中(2) 也可能挂在委托上（本地匹配还没做完），不能被委托挡在本地匹配之外。
            match search_status {
                SubAnimeSearchStatus::Searching => {
                    builder.push(Self::SEARCHING_CONDITION);
                }
                SubAnimeSearchStatus::NotSearch => {
                    builder.push("NOT ");
                    builder.push(Self::ON_MANDATE_EXISTS);
                    builder.push(" AND sa.search_status = 0");
                }
                other => {
                    builder.push("sa.search_status = ");
                    builder.push_bind(i32::from(other));
                }
            }
            has_condition = true;
        }
        if let Some(sub_status) = &query.sub_status {
            builder.push(if has_condition { " AND " } else { " WHERE " });
            // eps 是子查询计算出来的，这里复用同一个表达式
            // 总集数为 0 时这一季还没有可数的集数，不算完结，仍可参与匹配
            match sub_status {
                SubAnimeStatus::Completed => {
                    builder.push("(");
                    builder.push(Self::EPS_SUB_QUERY);
                    builder.push(" > 0 AND sa.progress >= ");
                    builder.push(Self::EPS_SUB_QUERY);
                    builder.push(")");
                }
                SubAnimeStatus::Enable => {
                    builder.push("(");
                    builder.push(Self::EPS_SUB_QUERY);
                    builder.push(" = 0 OR sa.progress < ");
                    builder.push(Self::EPS_SUB_QUERY);
                    builder.push(")");
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

    // 该番剧在搜索委托上的订阅：这次抓回来的资源只与它们匹配。
    // 这里不能按搜索状态取：等待中(1)、匹配中(2) 的订阅同样挂在委托上，它们也要拿到这次抓回来的资源。
    async fn list_by_mandate(&self, anime_id: i64) -> Result<Vec<SubAnimeProps>> {
        let mut builder = QueryBuilder::new(Self::BASE_SELECT_JOIN);
        builder.push(" WHERE sa.anime_id = ");
        builder.push_bind(anime_id);
        builder.push(" AND ");
        builder.push(Self::ON_MANDATE_EXISTS);
        builder.push(" AND (");
        builder.push(Self::EPS_SUB_QUERY);
        builder.push(" = 0 OR sa.progress < ");
        builder.push(Self::EPS_SUB_QUERY);
        builder.push(") GROUP BY sa.id");

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

    async fn delete_eps(&self, sub_anime_id: i64, ep_ids: &[i64]) -> Result<Vec<EpisodeProp>> {
        if ep_ids.is_empty() {
            return self.list_eps(sub_anime_id).await;
        }

        let mut tx = self.pool.begin().await?;

        let mut builder = QueryBuilder::new("DELETE FROM sub_anime_episode WHERE sub_anime_id = ");
        builder.push_bind(sub_anime_id);
        builder.push(" AND id IN (");
        let mut separated = builder.separated(", ");
        for ep_id in ep_ids {
            separated.push_bind(*ep_id);
        }
        separated.push_unseparated(")");
        builder.build().execute(&mut *tx).await?;

        let mut builder = QueryBuilder::new(Self::EPISODE_SELECT_JOIN);
        builder.push(" WHERE se.sub_anime_id = ");
        builder.push_bind(sub_anime_id);
        builder.push(" ORDER BY se.ep_num ASC");
        let rows = builder.build().fetch_all(&mut *tx).await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(Self::row_to_episode_prop(&row)?);
        }

        tx.commit().await?;
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use chrono::NaiveDate;
    use dashmap::DashMap;
    use regex::Regex;
    use sqlx::SqlitePool;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use crate::entity::cap::{RuleRepository, SubAnimeRepository};
    use crate::entity::model::{
        Episode, EpsiodeStatus, Rule, SubAnimeListQuery, SubAnimeProps, SubAnimeSearchStatus,
        SubAnimeStatus,
    };
    use crate::infra::regex::RegexRuleMatcher;
    use crate::infra::repository::client::{
        RuleSqliteClient, SearchMandateSqliteClient, SubAnimeSqliteClient,
    };

    struct Fixture {
        dir: tempfile::TempDir,
        pool: SqlitePool,
        client: SubAnimeSqliteClient,
        rule_client: RuleSqliteClient,
    }

    async fn setup() -> Fixture {
        let dir = tempfile::tempdir().expect("create temp dir failed");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(dir.path().join("subscription-test.db"))
                    .create_if_missing(true),
            )
            .await
            .expect("connect sqlite failed");

        let client = SubAnimeSqliteClient::new(pool.clone());
        let rule_client = RuleSqliteClient::new(
            pool.clone(),
            RegexRuleMatcher::new(Arc::new(DashMap::<String, Regex>::new())),
        );

        let mut tx = pool.begin().await.expect("begin schema tx failed");
        client
            .init_with_tx(&mut tx)
            .await
            .expect("init sub_anime schema failed");
        rule_client
            .init_with_tx(&mut tx)
            .await
            .expect("init rule schema failed");
        // 订阅查询要用搜索委托的表，夹具一起建
        SearchMandateSqliteClient::new(pool.clone())
            .init_with_tx(&mut tx)
            .await
            .expect("init search mandate schema failed");
        // subscription 不依赖 anime / resource crate，测试里按生产查询实际引用的列最小化建表，
        // 结构对齐 crates/anime/src/infra/repository/client.rs 与
        // crates/resource/src/infra/repository/client.rs。
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

        Fixture {
            dir,
            pool,
            client,
            rule_client,
        }
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

    async fn seed_resource(pool: &SqlitePool, hash: [u8; 20], title: &str, url: &str) {
        sqlx::query("INSERT INTO resource (info_hash, title, url) VALUES (?, ?, ?)")
            .bind(hash.to_vec())
            .bind(title)
            .bind(url)
            .execute(pool)
            .await
            .expect("insert resource failed");
    }

    async fn set_state(
        client: &SubAnimeSqliteClient,
        props: &SubAnimeProps,
        search_status: SubAnimeSearchStatus,
        progress: u32,
    ) {
        let mut data = props.data.clone();
        data.search_status = search_status;
        data.progress = progress;
        client
            .update_sub_anime(&data)
            .await
            .expect("update sub anime state failed");
    }

    fn query(
        anime_id: Option<i64>,
        space_id: Option<i64>,
        search_status: Option<SubAnimeSearchStatus>,
        sub_status: Option<SubAnimeStatus>,
        limit: Option<usize>,
    ) -> SubAnimeListQuery {
        SubAnimeListQuery {
            anime_id,
            space_id,
            search_status,
            sub_status,
            limit,
        }
    }

    fn ids(props: &[SubAnimeProps]) -> HashSet<i64> {
        props.iter().map(|p| p.data.id).collect()
    }

    /// 把订阅存到该番剧的搜索委托上：读出来就是搜索中
    async fn join_search_mandate(f: &Fixture, sub_anime_id: i64, anime_id: i64) {
        let mandate_id = sqlx::query("INSERT INTO search_mandate (anime_id) VALUES (?)")
            .bind(anime_id)
            .execute(&f.pool)
            .await
            .expect("insert search mandate failed")
            .last_insert_rowid();
        sqlx::query(
            "INSERT INTO search_mandate_sub_anime (search_mandate_id, sub_anime_id) VALUES (?, ?)",
        )
        .bind(mandate_id)
        .bind(sub_anime_id)
        .execute(&f.pool)
        .await
        .expect("insert mandate sub anime failed");
    }

    /// 三条订阅：
    /// a：space 9、番剧 100，在搜索委托上，进度 12/12 已播完
    /// b：space 9、番剧 200，等待中，进度 0/12 未播完
    /// c：space 7、番剧 100，等待中，进度 0/12 未播完
    async fn seed_three(f: &Fixture) -> (SubAnimeProps, SubAnimeProps, SubAnimeProps) {
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        seed_anime(&f.pool, 200, Some("2024-05-01"), Some(12), &["番B"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        let b = f.client.insert_sub_anime(9, 200).await.expect("insert b");
        let c = f.client.insert_sub_anime(7, 100).await.expect("insert c");
        set_state(&f.client, &a, SubAnimeSearchStatus::NotSearch, 12).await;
        join_search_mandate(f, a.data.id, 100).await;
        (a, b, c)
    }

    #[tokio::test]
    async fn fixture_database_file_lives_under_tempdir() {
        // 夹具把 SQLite 文件放在 tempdir 下，测试结束随目录一起删除
        let f = setup().await;
        assert!(f.dir.path().join("subscription-test.db").exists());
    }

    #[tokio::test]
    async fn list_without_filters_returns_parsed_props() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let all = f
            .client
            .list(&query(None, None, None, None, None))
            .await
            .expect("list all");

        assert_eq!(ids(&all), HashSet::from([a.data.id, b.data.id, c.data.id]));
        let row_a = all
            .iter()
            .find(|p| p.data.id == a.data.id)
            .expect("row a present");
        assert_eq!(row_a.extend.eps, 12);
        assert_eq!(
            row_a.extend.air_date,
            NaiveDate::from_ymd_opt(2024, 4, 1).expect("valid date")
        );
        assert_eq!(row_a.extend.titles, vec!["番A".to_string()]);
        assert_eq!(row_a.extend.rule_name, None);
        assert_eq!(row_a.data.search_status, SubAnimeSearchStatus::Searching);
        assert_eq!(row_a.data.progress, 12);
    }

    #[tokio::test]
    async fn list_filters_by_anime_id() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let rows = f
            .client
            .list(&query(Some(100), None, None, None, None))
            .await
            .expect("list by anime_id");

        assert_eq!(ids(&rows), HashSet::from([a.data.id, c.data.id]));
        assert!(!ids(&rows).contains(&b.data.id));
    }

    #[tokio::test]
    async fn list_filters_by_space_id() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let rows = f
            .client
            .list(&query(None, Some(9), None, None, None))
            .await
            .expect("list by space_id");

        assert_eq!(ids(&rows), HashSet::from([a.data.id, b.data.id]));
        assert!(!ids(&rows).contains(&c.data.id));
    }

    #[tokio::test]
    async fn list_filters_by_search_status() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        // 在搜索委托上的订阅算作搜索中
        let rows = f
            .client
            .list(&query(
                None,
                None,
                Some(SubAnimeSearchStatus::Searching),
                None,
                None,
            ))
            .await
            .expect("list by search_status");
        assert_eq!(ids(&rows), HashSet::from([a.data.id]));

        // 别的状态只取不在搜索委托上的订阅
        let rows = f
            .client
            .list(&query(
                None,
                None,
                Some(SubAnimeSearchStatus::Pending),
                None,
                None,
            ))
            .await
            .expect("list pending");
        assert_eq!(ids(&rows), HashSet::from([b.data.id, c.data.id]));

        for status in [
            SubAnimeSearchStatus::NotSearch,
            SubAnimeSearchStatus::Matching,
        ] {
            let rows = f
                .client
                .list(&query(None, None, Some(status), None, None))
                .await
                .expect("list status");
            assert!(rows.is_empty());
        }
    }

    #[tokio::test]
    async fn search_status_searching_comes_from_mandate_sub_anime() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");

        // 搜索中不存订阅表：写进去落库是不搜索
        set_state(&f.client, &a, SubAnimeSearchStatus::Searching, 0).await;
        let stored: i64 = sqlx::query_scalar("SELECT search_status FROM sub_anime WHERE id = ?")
            .bind(a.data.id)
            .fetch_one(&f.pool)
            .await
            .expect("read stored search_status");
        assert_eq!(stored, 0);
        let row = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find")
            .expect("exists");
        assert_eq!(row.data.search_status, SubAnimeSearchStatus::NotSearch);

        // 在搜索委托上，读出来是搜索中
        join_search_mandate(&f, a.data.id, 100).await;
        let row = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find")
            .expect("exists");
        assert_eq!(row.data.search_status, SubAnimeSearchStatus::Searching);

        // 委托完成后，读出来是订阅表里的取值
        sqlx::query("DELETE FROM search_mandate_sub_anime WHERE sub_anime_id = ?")
            .bind(a.data.id)
            .execute(&f.pool)
            .await
            .expect("delete mandate sub anime");
        let row = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find")
            .expect("exists");
        assert_eq!(row.data.search_status, SubAnimeSearchStatus::NotSearch);
    }

    #[tokio::test]
    async fn list_by_mandate_returns_waiting_enable_subscriptions() {
        let f = setup().await;
        let (_a, b, _c) = seed_three(&f).await;
        join_search_mandate(&f, b.data.id, 200).await;

        // 番剧 100 的订阅也在搜索委托上，但已经播完，不再参与匹配
        let rows = f.client.list_by_mandate(100).await.expect("list anime 100");
        assert!(rows.is_empty());

        let rows = f.client.list_by_mandate(200).await.expect("list anime 200");
        assert_eq!(ids(&rows), HashSet::from([b.data.id]));
    }

    #[tokio::test]
    async fn pending_sub_anime_on_mandate_still_waits_for_local_match() {
        // 订阅挂在搜索委托上、本地匹配却还没做完（订阅表里存着等待中(1)）：
        // 读出来还是等待中，本地匹配也能按等待中把它取出来
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        set_state(&f.client, &a, SubAnimeSearchStatus::Pending, 0).await;
        join_search_mandate(&f, a.data.id, 100).await;

        let row = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find")
            .expect("exists");
        assert_eq!(row.data.search_status, SubAnimeSearchStatus::Pending);

        let rows = f
            .client
            .list(&query(
                None,
                None,
                Some(SubAnimeSearchStatus::Pending),
                None,
                Some(1),
            ))
            .await
            .expect("list pending");
        assert_eq!(ids(&rows), HashSet::from([a.data.id]));

        // 没做完本地匹配，不算搜索中(3)
        let rows = f
            .client
            .list(&query(
                None,
                None,
                Some(SubAnimeSearchStatus::Searching),
                None,
                None,
            ))
            .await
            .expect("list searching");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn delete_removes_mandate_sub_anime_rows() {
        let f = setup().await;
        let (a, _b, _c) = seed_three(&f).await;

        f.client.delete(a.data.id).await.expect("delete");

        let left: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM search_mandate_sub_anime WHERE sub_anime_id = ?",
        )
        .bind(a.data.id)
        .fetch_one(&f.pool)
        .await
        .expect("count mandate sub anime");
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn list_filters_by_sub_status_completed() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let rows = f
            .client
            .list(&query(
                None,
                None,
                None,
                Some(SubAnimeStatus::Completed),
                None,
            ))
            .await
            .expect("list completed");

        assert_eq!(ids(&rows), HashSet::from([a.data.id]));
        assert!(!ids(&rows).contains(&b.data.id));
        assert!(!ids(&rows).contains(&c.data.id));
    }

    #[tokio::test]
    async fn list_filters_by_sub_status_enable() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let rows = f
            .client
            .list(&query(None, None, None, Some(SubAnimeStatus::Enable), None))
            .await
            .expect("list enable");

        assert_eq!(ids(&rows), HashSet::from([b.data.id, c.data.id]));
        assert!(!ids(&rows).contains(&a.data.id));
    }

    #[tokio::test]
    async fn list_applies_limit() {
        let f = setup().await;
        let (a, b, c) = seed_three(&f).await;

        let rows = f
            .client
            .list(&query(None, None, None, None, Some(1)))
            .await
            .expect("list limit 1");

        // 三条订阅里只返回一条，且必然是刚造的其中一条
        assert_eq!(rows.len(), 1);
        let only = &rows[0].data.id;
        assert!(only == &a.data.id || only == &b.data.id || only == &c.data.id);
    }

    #[tokio::test]
    async fn list_sub_status_enable_keeps_zero_eps_subscription_for_match() {
        // 总集数为 0：anime_season 没有 Bangumi 行时 eps 被 COALESCE 成 0。
        // 这一季还没有可数的集数，不算完结，按 Enable 过滤仍然查得到。
        let f = setup().await;
        seed_anime(&f.pool, 300, Some("2024-04-01"), None, &["番C"]).await;
        let c = f.client.insert_sub_anime(9, 300).await.expect("insert c");
        assert_eq!(c.extend.eps, 0);

        let enable = f
            .client
            .list(&query(None, None, None, Some(SubAnimeStatus::Enable), None))
            .await
            .expect("list enable");
        assert_eq!(ids(&enable), HashSet::from([c.data.id]));

        let completed = f
            .client
            .list(&query(
                None,
                None,
                None,
                Some(SubAnimeStatus::Completed),
                None,
            ))
            .await
            .expect("list completed");
        assert!(completed.is_empty());
    }

    #[tokio::test]
    async fn list_sub_status_enable_keeps_zero_eps_subscription_with_progress() {
        // 总集数为 0 但已经记了进度：仍不算完结，继续参与匹配
        let f = setup().await;
        seed_anime(&f.pool, 300, Some("2024-04-01"), None, &["番C"]).await;
        let c = f.client.insert_sub_anime(9, 300).await.expect("insert c");
        set_state(&f.client, &c, SubAnimeSearchStatus::NotSearch, 5).await;

        let enable = f
            .client
            .list(&query(None, None, None, Some(SubAnimeStatus::Enable), None))
            .await
            .expect("list enable");
        assert_eq!(ids(&enable), HashSet::from([c.data.id]));
    }

    #[tokio::test]
    async fn get_one_undownload_ep_returns_pending_episode() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        seed_resource(&f.pool, [1u8; 20], "番A 第01集", "http://res/1").await;
        seed_resource(&f.pool, [2u8; 20], "番A 第02集", "http://res/2").await;

        let pending = Episode {
            sub_anime_id: a.data.id,
            resource_id: [1u8; 20],
            status: EpsiodeStatus::Pending,
            ep_num: Some(1.0),
        };
        let done = Episode {
            sub_anime_id: a.data.id,
            resource_id: [2u8; 20],
            status: EpsiodeStatus::Downloaded,
            ep_num: Some(2.0),
        };
        f.client
            .update_sub_anime_progress(&a.data, &[pending, done])
            .await
            .expect("seed episodes");

        let ep = f
            .client
            .get_one_undownload_ep()
            .await
            .expect("query pending")
            .expect("has pending episode");

        assert_eq!(ep.data.ep.status, EpsiodeStatus::Pending);
        assert_eq!(ep.data.ep.resource_id, [1u8; 20]);
        assert_eq!(ep.data.ep.ep_num, Some(1.0));
        assert_eq!(ep.extend.title, "番A 第01集");
        assert_eq!(ep.extend.url, "http://res/1");
        assert_eq!(ep.extend.anime_id, 100);
        assert_eq!(ep.extend.space_id, 9);
    }

    #[tokio::test]
    async fn get_one_undownload_ep_returns_none_when_all_downloaded() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        seed_resource(&f.pool, [2u8; 20], "番A 第01集", "http://res/1").await;

        let done = Episode {
            sub_anime_id: a.data.id,
            resource_id: [2u8; 20],
            status: EpsiodeStatus::Downloaded,
            ep_num: Some(1.0),
        };
        f.client
            .update_sub_anime_progress(&a.data, &[done])
            .await
            .expect("seed episodes");

        assert!(
            f.client
                .get_one_undownload_ep()
                .await
                .expect("query pending")
                .is_none()
        );
    }

    async fn insert_rule(f: &Fixture, space_id: i64, name: &str) -> i64 {
        f.rule_client
            .insert(&Rule {
                space_id,
                name: name.to_string(),
                order: 1,
                pattern: "番A".to_string(),
            })
            .await
            .expect("insert rule")
            .id
    }

    async fn seed_one_episode(f: &Fixture, sub_anime_id: i64, hash: [u8; 20]) {
        seed_resource(&f.pool, hash, "番A 第01集", "http://res/1").await;
        let ep = Episode {
            sub_anime_id,
            resource_id: hash,
            status: EpsiodeStatus::Pending,
            ep_num: Some(1.0),
        };
        let props = f
            .client
            .find_sub_anime(sub_anime_id)
            .await
            .expect("find sub anime")
            .expect("sub anime exists");
        f.client
            .update_sub_anime_progress(&props.data, &[ep])
            .await
            .expect("seed episode");
    }

    #[tokio::test]
    async fn binding_rule_and_clear_eps_updates_rule_and_removes_episodes() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        seed_one_episode(&f, a.data.id, [1u8; 20]).await;
        assert_eq!(
            f.client
                .list_eps(a.data.id)
                .await
                .expect("list eps before")
                .len(),
            1
        );

        let rule_id = insert_rule(&f, 9, "规则A").await;
        f.client
            .binding_rule_and_clear_eps(a.data.id, rule_id)
            .await
            .expect("bind rule and clear eps");

        let after = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find after")
            .expect("still exists");
        assert_eq!(after.data.rule_id, Some(rule_id));
        assert_eq!(after.data.progress, 0);
        assert!(
            f.client
                .list_eps(a.data.id)
                .await
                .expect("list eps after")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn binding_rule_and_clear_eps_rejects_space_mismatch() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        seed_one_episode(&f, a.data.id, [1u8; 20]).await;
        // 规则属于 space 7，订阅属于 space 9，空间不匹配
        let rule_id = insert_rule(&f, 7, "规则B").await;

        let err = f
            .client
            .binding_rule_and_clear_eps(a.data.id, rule_id)
            .await
            .expect_err("space mismatch must fail");
        assert!(err.to_string().contains("binding failed"));

        let after = f
            .client
            .find_sub_anime(a.data.id)
            .await
            .expect("find after")
            .expect("still exists");
        assert_eq!(after.data.rule_id, None);
        assert_eq!(
            f.client
                .list_eps(a.data.id)
                .await
                .expect("list eps after")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn binding_rule_and_clear_eps_rejects_missing_rule() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");

        let err = f
            .client
            .binding_rule_and_clear_eps(a.data.id, 999)
            .await
            .expect_err("missing rule must fail");
        assert!(err.to_string().contains("binding failed"));
    }

    #[tokio::test]
    async fn delete_eps_removes_only_the_given_episodes() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        seed_anime(&f.pool, 200, Some("2024-04-01"), Some(12), &["番B"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        let b = f.client.insert_sub_anime(9, 200).await.expect("insert b");
        seed_one_episode(&f, a.data.id, [1u8; 20]).await;
        seed_one_episode(&f, a.data.id, [2u8; 20]).await;
        seed_one_episode(&f, b.data.id, [3u8; 20]).await;

        let first_ep = f
            .client
            .list_eps(a.data.id)
            .await
            .expect("list eps of a")
            .into_iter()
            .find(|i| i.data.ep.resource_id == [1u8; 20])
            .expect("episode of the first resource")
            .data
            .id;

        let left = f
            .client
            .delete_eps(a.data.id, &[first_ep])
            .await
            .expect("delete eps failed");

        assert_eq!(
            left.len(),
            1,
            "delete_eps should return the remaining episodes"
        );
        assert_eq!(left[0].data.ep.resource_id, [2u8; 20]);
        assert_eq!(
            f.client
                .list_eps(a.data.id)
                .await
                .expect("list eps of a")
                .len(),
            1,
            "delete_eps should only delete the given episodes"
        );
        assert_eq!(
            f.client
                .list_eps(b.data.id)
                .await
                .expect("list eps of b")
                .len(),
            1,
            "delete_eps should keep episodes of other subscriptions"
        );
    }

    #[tokio::test]
    async fn delete_eps_with_empty_episode_ids_deletes_nothing() {
        let f = setup().await;
        seed_anime(&f.pool, 100, Some("2024-04-01"), Some(12), &["番A"]).await;
        let a = f.client.insert_sub_anime(9, 100).await.expect("insert a");
        seed_one_episode(&f, a.data.id, [1u8; 20]).await;

        let left = f
            .client
            .delete_eps(a.data.id, &[])
            .await
            .expect("delete eps with empty episode ids failed");

        assert_eq!(
            left.len(),
            1,
            "delete_eps should return current episodes for empty episode ids"
        );
        assert_eq!(
            f.client
                .list_eps(a.data.id)
                .await
                .expect("list eps of a")
                .len(),
            1,
            "delete_eps should delete nothing for empty episode ids"
        );
    }
}
