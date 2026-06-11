use std::{pin::Pin, sync::Arc};

use common::shared::{error::Error, str::nfkc_to_lowercase};
use feed::entity::model::FeedItem;
use futures::{Stream, StreamExt};

use crate::entity::{
    cap::ResourceRepository,
    model::{ResourceBaseData, ResourceQuery},
    resource_entity::ResourceEntity,
};

#[derive(Clone)]
pub struct Resources {
    repo: Arc<dyn ResourceRepository>,
}

impl Resources {
    pub fn new(repo: Arc<dyn ResourceRepository>) -> Self {
        Self { repo }
    }

    pub fn stream<'a>(
        &'a self,
        query: &'a ResourceQuery,
    ) -> Pin<Box<dyn Stream<Item = Result<ResourceEntity, Error>> + Send + 'a>> {
        let raw_stream = self.repo.stream(query);
        let converted = raw_stream.map(|item| {
            item.map(|prop| ResourceEntity::new(prop.data))
                .map_err(|e| Error::external("resources stream res entity error", e))
        });
        Box::pin(converted)
    }

    pub async fn just_save(&self, items: Vec<FeedItem>) -> Result<(), Error> {
        let data = items
            .into_iter()
            .map(|i| {
                let match_title = nfkc_to_lowercase(&i.title);
                ResourceBaseData {
                    title: i.title,
                    match_title,
                    url: i.resource_url,
                    info_hash: i.info_hash,
                    published_at: i.published_at,
                }
            })
            .collect::<Vec<_>>();
        self.repo
            .insert_or_skip(data)
            .await
            .map_err(|e| Error::external("resources save failed", e))?;
        Ok(())
    }

    // save
    // 保存并返回新资源
    pub async fn save(&self, items: Vec<FeedItem>) -> Result<Vec<ResourceEntity>, Error> {
        let data = items
            .into_iter()
            .map(|i| {
                let match_title = nfkc_to_lowercase(&i.title);
                ResourceBaseData {
                    title: i.title,
                    match_title,
                    url: i.resource_url,
                    info_hash: i.info_hash,
                    published_at: i.published_at,
                }
            })
            .collect::<Vec<_>>();
        let res = self
            .repo
            .insert_or_skip_return_new(data)
            .await
            .map_err(|e| Error::external("resources save and get new res failed", e))?;
        Ok(res
            .into_iter()
            .map(|i| ResourceEntity::new(i.data))
            .collect())
    }
}
