use crate::{
    entity::{
        cap::SearchMandateRepository,
        model::{Mandate, SearchMandateBaseData, SearchMandateProp},
    },
    infra::repository::client::SearchMandateSqliteClient,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{QueryBuilder, Row};

#[async_trait]
impl SearchMandateRepository for SearchMandateSqliteClient {
    async fn get_one(&self, block_feed_ids: &[i64]) -> Result<Option<SearchMandateProp>> {
        let mut qb = QueryBuilder::new(
            "SELECT p.id, m.anime_id, p.feed_id, p.url 
            FROM search_pool p 
            JOIN search_mandate m ON p.search_mandate_id = m.id",
        );

        if !block_feed_ids.is_empty() {
            qb.push(" WHERE p.feed_id NOT IN (");
            let mut separated = qb.separated(", ");
            for id in block_feed_ids {
                separated.push_bind(id);
            }
            separated.push_unseparated(") ");
        }

        qb.push(" ORDER BY RANDOM() LIMIT 1");

        let row = qb
            .build()
            .fetch_optional(&self.pool)
            .await
            .context("failed to query random mandate")?;

        match row {
            None => Ok(None),
            Some(r) => Self::parse_row(&r).map(Some),
        }
    }
    async fn delete_and_count(&self, id: i64, anime_id: i64) -> Result<u64> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin transaction failed")?;

        let mut delete_qb = QueryBuilder::new("DELETE FROM search_pool WHERE id = ");
        delete_qb.push_bind(id);
        delete_qb
            .push(" AND search_mandate_id IN (SELECT id FROM search_mandate WHERE anime_id = ");
        delete_qb.push_bind(anime_id);
        delete_qb.push(") RETURNING search_mandate_id");

        let delete_row = delete_qb
            .build()
            .fetch_optional(&mut *tx)
            .await
            .context("delete pool record failed")?;

        if let Some(r) = delete_row {
            let mandate_id: i64 = r.get(0);
            let mut delete_mandate_qb = QueryBuilder::new("DELETE FROM search_mandate WHERE id = ");
            delete_mandate_qb.push_bind(mandate_id);
            delete_mandate_qb
                .push(" AND NOT EXISTS (SELECT 1 FROM search_pool WHERE search_mandate_id = ");
            delete_mandate_qb.push_bind(mandate_id);
            delete_mandate_qb.push(")");

            delete_mandate_qb
                .build()
                .execute(&mut *tx)
                .await
                .context("clean mandate record failed")?;
        }

        let mut count_qb = QueryBuilder::new(
            "SELECT COUNT(*) FROM search_pool WHERE search_mandate_id IN (SELECT id FROM search_mandate WHERE anime_id = ",
        );
        count_qb.push_bind(anime_id);
        count_qb.push(")");

        let row = count_qb
            .build()
            .fetch_one(&mut *tx)
            .await
            .context("count remaining mandates failed")?;

        let remaining: i64 = row.get(0);

        tx.commit().await.context("commit transaction failed")?;

        Ok(remaining as u64)
    }

    async fn save(&self, data: &[Mandate]) -> Result<Vec<SearchMandateProp>> {
        if data.is_empty() {
            return Ok(vec![]);
        }

        let anime_id = data[0].anime_id;
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin transaction failed")?;

        // 检查该 anime_id 是否已存在
        let mut exists_qb =
            QueryBuilder::new("SELECT EXISTS(SELECT 1 FROM search_mandate WHERE anime_id = ");
        exists_qb.push_bind(anime_id);
        exists_qb.push(")");

        let row = exists_qb
            .build()
            .fetch_one(&mut *tx)
            .await
            .context("check anime_id existence failed")?;
        let exists: bool = row.get(0);

        if exists {
            return Ok(vec![]);
        }

        let mut insert_mandate_qb =
            QueryBuilder::new("INSERT INTO search_mandate (anime_id) VALUES (");
        insert_mandate_qb.push_bind(anime_id);
        insert_mandate_qb.push(")");

        let res = insert_mandate_qb
            .build()
            .execute(&mut *tx)
            .await
            .context("insert search_mandate failed")?;

        let mandate_id = res.last_insert_rowid();

        // 逐条插入 search_pool 并构造结果
        let mut props = Vec::with_capacity(data.len());
        for m in data {
            let mut insert_pool_qb = QueryBuilder::new(
                "INSERT INTO search_pool (search_mandate_id, feed_id, url) VALUES (",
            );
            insert_pool_qb.push_bind(mandate_id);
            insert_pool_qb.push(", ");
            insert_pool_qb.push_bind(m.feed_id);
            insert_pool_qb.push(", ");
            insert_pool_qb.push_bind(&m.url);
            insert_pool_qb.push(")");

            let res = insert_pool_qb
                .build()
                .execute(&mut *tx)
                .await
                .context("insert search_pool failed")?;

            let pool_id = res.last_insert_rowid();

            props.push(SearchMandateProp {
                data: SearchMandateBaseData {
                    id: pool_id,
                    mandata: Mandate {
                        anime_id: m.anime_id,
                        feed_id: m.feed_id,
                        url: m.url.clone(),
                    },
                },
            });
        }

        tx.commit().await.context("commit transaction failed")?;

        Ok(props)
    }

    async fn count(&self) -> Result<u64> {
        let mut qb = QueryBuilder::new("SELECT COUNT(*) FROM search_pool");
        let row = qb
            .build()
            .fetch_one(&self.pool)
            .await
            .context("count mandates failed")?;

        let count: i64 = row.get(0);
        Ok(count as u64)
    }

    async fn delete(&self, id: i64) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("begin transaction failed")?;

        let mut delete_pool_qb = QueryBuilder::new("DELETE FROM search_pool WHERE id = ");
        delete_pool_qb.push_bind(id);
        delete_pool_qb.push(" RETURNING search_mandate_id");

        let row = delete_pool_qb
            .build()
            .fetch_optional(&mut *tx)
            .await
            .context("delete pool record failed")?;

        let mandate_id: i64 = match row {
            Some(r) => r.get(0),
            None => return Ok(()),
        };

        let mut delete_mandate_qb = QueryBuilder::new("DELETE FROM search_mandate WHERE id = ");
        delete_mandate_qb.push_bind(mandate_id);
        delete_mandate_qb
            .push(" AND NOT EXISTS (SELECT 1 FROM search_pool WHERE search_mandate_id = ");
        delete_mandate_qb.push_bind(mandate_id);
        delete_mandate_qb.push(")");

        delete_mandate_qb
            .build()
            .execute(&mut *tx)
            .await
            .context("clean mandate record failed")?;

        tx.commit().await.context("commit transaction failed")?;
        Ok(())
    }
}
