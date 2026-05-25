use std::sync::Arc;

use domain::{
    feed::{ResourceId, ResourceRepository},
    shared::error::DomainError,
};
use user::gateway::EpochClock;

use crate::{contracts::FeedData, entity::FeedEntity, resource_entity::ResourceEntity};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResourceListQuery {
    pub since: Option<i64>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct Resources {
    resource_repository: Arc<dyn ResourceRepository>,
    clock: Arc<dyn EpochClock>,
}

impl Resources {
    pub fn new(
        resource_repository: Arc<dyn ResourceRepository>,
        clock: Arc<dyn EpochClock>,
    ) -> Self {
        Self {
            resource_repository,
            clock,
        }
    }

    pub async fn load(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Option<ResourceEntity>, DomainError> {
        let Some(resource) = self.resource_repository.find_resource(resource_id).await? else {
            return Ok(None);
        };
        let sources = self
            .resource_repository
            .list_resource_sources(resource_id)
            .await?;
        Ok(Some(ResourceEntity::new(resource, sources)?))
    }

    pub async fn save(&self, entity: &ResourceEntity) -> Result<(), DomainError> {
        self.resource_repository
            .save_resource(entity.read_data())
            .await?;
        for source in entity.read_sources() {
            self.resource_repository
                .save_resource_source(source)
                .await?;
        }
        Ok(())
    }

    pub async fn list(&self, query: ResourceListQuery) -> Result<Vec<ResourceEntity>, DomainError> {
        match (query.since, query.keywords) {
            (Some(since), None) => self.list_recent(since).await,
            (None, Some(keywords)) => self.list_keywords(&keywords).await,
            _ => Err(DomainError::InvariantViolation(
                "resource list query must specify exactly one filter",
            )),
        }
    }

    pub async fn ingest(&self, feed_data: FeedData) -> Result<Vec<ResourceEntity>, DomainError> {
        let mut new_resources = Vec::new();

        for item in feed_data.items {
            if FeedEntity::is_collection_pack(&item.title) {
                continue;
            }
            let (resource, saved) = match self.ingest_feed_item(&feed_data.source_key, item).await {
                Ok(result) => result,
                Err(error) => {
                    tracing::error!(
                        source_key = %feed_data.source_key,
                        %error,
                        "save feed item failed, skipping item"
                    );
                    continue;
                }
            };
            if !saved {
                continue;
            }
            new_resources.push(resource);
        }

        Ok(new_resources)
    }

    async fn list_recent(&self, since: i64) -> Result<Vec<ResourceEntity>, DomainError> {
        let mut resources = Vec::new();
        for resource in self.resource_repository.latest_resources(since).await? {
            let resource_id = resource.id.clone();
            let sources = self
                .resource_repository
                .list_resource_sources(&resource_id)
                .await?;
            resources.push(ResourceEntity::new(resource, sources)?);
        }
        Ok(resources)
    }

    async fn list_keywords(&self, keywords: &[String]) -> Result<Vec<ResourceEntity>, DomainError> {
        let mut resources = Vec::new();
        for resource in self.resource_repository.search_resources(keywords).await? {
            let resource_id = resource.id.clone();
            let sources = self
                .resource_repository
                .list_resource_sources(&resource_id)
                .await?;
            resources.push(ResourceEntity::new(resource, sources)?);
        }
        Ok(resources)
    }

    async fn ingest_feed_item(
        &self,
        source_key: &str,
        item: crate::contracts::FetchedFeedItem,
    ) -> Result<(ResourceEntity, bool), DomainError> {
        let now = self.clock.now_epoch_seconds();
        let incoming = ResourceEntity::from_fetched_feed_item(source_key, item, now)?;
        let resource_id = incoming.read_data().id.clone();
        let source =
            incoming
                .read_sources()
                .first()
                .cloned()
                .ok_or(DomainError::InvariantViolation(
                    "resource source is missing",
                ))?;

        if let Some(mut entity) = self.load(&resource_id).await? {
            let changed = entity.include_source(source)?;
            if changed {
                self.save(&entity).await?;
            }
            return Ok((entity, false));
        }

        self.save(&incoming).await?;
        Ok((incoming, true))
    }
}
