use anyhow::Result;
use async_trait::async_trait;
use sqlx::QueryBuilder;

use crate::{
    entity::{
        cap::FeedRepository,
        model::{FeedBaseData, FeedListQuery, FeedMetadata, FeedProp, FeedType},
    },
    infra::repository::client::FeedSqliteClient,
};

#[async_trait]
impl FeedRepository for FeedSqliteClient {
    async fn list(&self, query: &FeedListQuery) -> Result<Vec<FeedProp>> {
        let mut builder =
            QueryBuilder::new("SELECT id, title, site_url, search_url, source_key FROM feed");

        match query.feed_type {
            FeedType::Site => {
                builder.push(" WHERE site_url IS NOT NULL");
            }
            FeedType::Search => {
                builder.push(" WHERE search_url IS NOT NULL");
            }
            FeedType::Both => {}
        };

        let rows = builder
            .build_query_as::<(i64, String, Option<String>, Option<String>, String)>()
            .fetch_all(&self.pool)
            .await?;

        let result = rows
            .into_iter()
            .map(|(id, title, site_url, search_url, source_key)| FeedProp {
                data: FeedBaseData {
                    id,
                    metadata: FeedMetadata {
                        title,
                        site_url,
                        search_url,
                        source_key,
                    },
                },
            })
            .collect();

        Ok(result)
    }

    async fn insert(&self, entity: &FeedMetadata) -> Result<FeedProp> {
        let row = sqlx::query_as::<_, (i64,)>(
            "INSERT INTO feed (title, site_url, search_url, source_key)
             VALUES (?, ?, ?, ?)
             RETURNING id",
        )
        .bind(&entity.title)
        .bind(&entity.site_url)
        .bind(&entity.search_url)
        .bind(&entity.source_key)
        .fetch_one(&self.pool)
        .await?;

        Ok(FeedProp {
            data: FeedBaseData {
                id: row.0,
                metadata: entity.clone(),
            },
        })
    }

    async fn update(&self, entity: &FeedBaseData) -> Result<()> {
        sqlx::query(
            "UPDATE feed
             SET title = ?,
                 site_url = ?,
                 search_url = ?,
                 source_key = ?,
                 updated_at = unixepoch()
             WHERE id = ?",
        )
        .bind(&entity.metadata.title)
        .bind(&entity.metadata.site_url)
        .bind(&entity.metadata.search_url)
        .bind(&entity.metadata.source_key)
        .bind(entity.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM feed WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get(&self, id: i64) -> Result<Option<FeedProp>> {
        let (id, title, site_url, search_url, source_key) =
            sqlx::query_as::<_, (i64, String, Option<String>, Option<String>, String)>(
                "SELECT id, title, site_url, search_url, source_key FROM feed WHERE id = ?",
            )
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(Some(FeedProp {
            data: FeedBaseData {
                id,
                metadata: FeedMetadata {
                    title,
                    site_url,
                    search_url,
                    source_key,
                },
            },
        }))
    }
}
