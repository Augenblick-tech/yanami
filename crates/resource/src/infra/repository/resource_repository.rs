use std::{collections::HashMap, pin::Pin};

use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use feed::infra::feed::FeedItemRepository;
use futures::{Stream, StreamExt};
use sqlx::{QueryBuilder, Row};

use crate::{
    entity::{
        cap::ResourceRepository,
        model::{ResourceBaseData, ResourceProp, ResourceQuery},
    },
    infra::repository::client::ResourceSqliteClient,
};

const BATCH_SIZE: usize = 100;

#[async_trait]
impl ResourceRepository for ResourceSqliteClient {
    fn stream<'a>(
        &'a self,
        query: &'a ResourceQuery,
    ) -> Pin<Box<dyn Stream<Item = Result<ResourceProp>> + Send + 'a>> {
        let stream = try_stream! {
            let mut qb = QueryBuilder::new(
                "SELECT info_hash, title, match_title, url, published_at FROM resource WHERE 1=1"
            );

            // 1. 优先拼接能走 B-Tree 索引的范围过滤
            if let Some(start_at) = query.start_at {
                qb.push(" AND published_at >= ");
                qb.push_bind(start_at);
            }
            if let Some(end_at) = query.end_at {
                qb.push(" AND published_at <= ");
                qb.push_bind(end_at);
            }

            // 2. 无法利用普通索引的模糊检索后置
            if let Some(ref keywords) = query.keywords {
                for kw in keywords {
                    if !kw.trim().is_empty() {
                        let pattern = format!("%{}%", kw.trim());
                        qb.push(" AND (title LIKE ");
                        // 注意：这里必须传值 (Owned String) 以交出所有权，绝对不能传 &pattern 引用
                        qb.push_bind(pattern.clone());
                        qb.push(" OR match_title LIKE ");
                        qb.push_bind(pattern);
                        qb.push(")");
                    }
                }
            }

            qb.push(" ORDER BY published_at DESC");

            if let Some(limit) = query.limit {
                qb.push(" LIMIT ");
                qb.push_bind(limit);
            }
            if let Some(offset) = query.offset {
                qb.push(" OFFSET ");
                qb.push_bind(offset);
            }

            // 此时 qb 的所有权存在于 try_stream! 生成的 Future/Stream 状态机堆内存中
            // qb.build() 的借用生命周期与整个 Stream 完全一致，不再产生悬垂引用
            let mut rows = qb.build().fetch(&self.pool);

            while let Some(row_res) = rows.next().await {
                let row = row_res?;
                let data = Self::parse_resource_row(&row)?;
                yield ResourceProp { data };
            }
        };

        Box::pin(stream)
    }

    async fn insert_or_skip(&self, items: Vec<ResourceBaseData>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;

        for chunk in items.chunks(BATCH_SIZE) {
            self.batch_insert_resource(&mut tx, chunk, false).await?;
            self.batch_insert_url_hash(&mut tx, chunk).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn insert_or_skip_return_new(
        &self,
        items: Vec<ResourceBaseData>,
    ) -> Result<Vec<ResourceProp>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(items.len());

        let mut tx = self.pool.begin().await?;
        for chunk in items.chunks(BATCH_SIZE) {
            let inserted = self.batch_insert_resource(&mut tx, chunk, true).await?;
            self.batch_insert_url_hash(&mut tx, chunk).await?;

            results.extend(inserted);
        }

        tx.commit().await?;
        Ok(results)
    }
}

#[async_trait]
impl FeedItemRepository for ResourceSqliteClient {
    async fn get_url_info_hash(&self, urls: Vec<&str>) -> Result<HashMap<String, [u8; 20]>> {
        if urls.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query_builder =
            QueryBuilder::new("SELECT url, info_hash FROM resource_url_info_hash WHERE url IN (");
        let mut separated = query_builder.separated(", ");
        for url in urls {
            separated.push_bind(url);
        }
        query_builder.push(")");

        let query = query_builder.build();
        let rows = query.fetch_all(&self.pool).await?;

        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            let url: String = row.try_get("url")?;
            let hash_blob: Vec<u8> = row.try_get("info_hash")?;
            let hash: [u8; 20] = hash_blob
                .try_into()
                .map_err(|_| anyhow::anyhow!("info_hash length is not 20"))?;
            map.insert(url, hash);
        }

        Ok(map)
    }
}
