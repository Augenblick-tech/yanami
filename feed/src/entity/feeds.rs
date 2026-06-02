use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{
    cap::{FeedFetcher, FeedIDGenerator, FeedRepository},
    feed_entity::FeedEntity,
};

#[derive(Clone)]
pub struct Feeds {
    repo: Arc<dyn FeedRepository>,
    feed_id_gen: Arc<dyn FeedIDGenerator>,
    fetch_cap: Arc<dyn FeedFetcher>,
}

impl Feeds {
    pub fn new(
        repo: Arc<dyn FeedRepository>,
        feed_id_gen: Arc<dyn FeedIDGenerator>,
        fetch_cap: Arc<dyn FeedFetcher>,
    ) -> Self {
        Self {
            repo,
            feed_id_gen,
            fetch_cap,
        }
    }

    pub async fn list(&self) -> Result<Vec<FeedEntity>, DomainError> {
        Ok(self.repo.list().await?)
    }

    pub async fn create(
        &self,
        title: String,
        site_url: Option<String>,
        search_url: Option<String>,
    ) -> Result<FeedEntity, DomainError> {
        let id = self.feed_id_gen.next_id().await?;
        let mut entity = FeedEntity::new(id, title, site_url, search_url, None)?;
        entity.verify(self.fetch_cap.as_ref()).await?;
        self.repo.insert(&entity).await?;
        Ok(entity)
    }

    pub async fn save(&self, entity: &mut FeedEntity) -> Result<(), DomainError> {
        entity.verify(self.fetch_cap.as_ref()).await?;
        self.repo.update(&entity).await?;
        Ok(())
    }
}
