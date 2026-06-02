use async_trait::async_trait;
use domain::shared::error::DomainError;

use crate::entity::feed_entity::FeedEntity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedData {
    pub source_key: String,
    pub items: Vec<FeedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub title: String,
    pub source_url: String,
    pub torrent_content: Option<Vec<u8>>,
    pub published_at: Option<i64>,
}

#[async_trait]
pub trait FeedFetcher: Send + Sync {
    async fn fetch_url(&self, url: &str) -> Result<FeedData, DomainError>;
}

#[async_trait]
pub trait FeedUpdater: Send + Sync {
    async fn set_site_url(&self, feed_id: &str, site_url: &str) -> Result<(), DomainError>;
    async fn set_search_url(&self, feed_id: &str, search_url: &str) -> Result<(), DomainError>;
    async fn set_source_key(&self, feed_id: &str, source_key: &str) -> Result<(), DomainError>;
}

#[async_trait]
pub trait FeedRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<FeedEntity>, DomainError>;
    async fn insert(&self, entity: &FeedEntity) -> Result<(), DomainError>;
    async fn update(&self, entity: &FeedEntity) -> Result<(), DomainError>;
}

#[async_trait]
pub trait FeedIDGenerator: Send + Sync {
    async fn next_id(&self) -> Result<String, DomainError>;
}
