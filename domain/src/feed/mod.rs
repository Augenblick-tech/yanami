use std::sync::Arc;

use async_trait::async_trait;

use crate::{shared::biz::BizContext, shared::error::DomainError, space::SpaceId};

pub mod capability;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeedSourceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSource {
    pub id: FeedSourceId,
    pub title: String,
    pub site_url: Option<String>,
    pub search_url: Option<String>,
    pub source_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub id: ResourceId,
    pub title: String,
    pub source_url: String,
    pub source_key: String,
    pub published_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSource {
    pub resource_id: ResourceId,
    pub source_key: String,
    pub source_url: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedFetchResult {
    pub saved_count: usize,
    pub new_resource_ids: Vec<ResourceId>,
    pub new_resources: Vec<Resource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSearchResult {
    pub saved_count: usize,
    pub resources: Vec<Resource>,
}

#[async_trait]
pub trait SpaceFeedRepository: Send + Sync {
    async fn find_space_feeds(&self, space_id: SpaceId) -> Result<Vec<FeedSource>, DomainError>;

    async fn list_space_feeds(&self) -> Result<Vec<FeedSource>, DomainError>;

    async fn save_space_feed(
        &self,
        space_id: SpaceId,
        source: &FeedSource,
    ) -> Result<(), DomainError>;

    async fn update_space_feed_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError>;

    async fn delete_space_feed(
        &self,
        space_id: SpaceId,
        source_id: &FeedSourceId,
    ) -> Result<(), DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn SpaceFeedRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "space feed repository does not support biz context",
        ))
    }
}

#[async_trait]
pub trait ResourceRepository: Send + Sync {
    async fn find_resource(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Option<Resource>, DomainError>;

    async fn list_resource_sources(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Vec<ResourceSource>, DomainError>;

    async fn save_resource(&self, resource: &Resource) -> Result<(), DomainError>;

    async fn save_resource_source(&self, source: &ResourceSource) -> Result<(), DomainError>;

    async fn latest_resources(&self, since: i64) -> Result<Vec<Resource>, DomainError>;

    async fn search_resources(&self, keywords: &[String]) -> Result<Vec<Resource>, DomainError>;
}
