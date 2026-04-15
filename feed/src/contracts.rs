use std::{future::Future, pin::Pin};

use async_trait::async_trait;
use domain::{
    feed::{FeedSource, FeedSourceId},
    shared::error::DomainError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedFeedItem {
    pub title: String,
    pub source_url: String,
    pub torrent_content: Option<Vec<u8>>,
    pub published_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedData {
    pub source_key: String,
    pub items: Vec<FetchedFeedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFeedSource {
    pub source_key: String,
}

#[async_trait]
pub trait FeedFetcher: Send + Sync {
    async fn fetch(&self, source: &FeedSource) -> Result<FeedData, DomainError>;

    async fn search(&self, source: &FeedSource, keyword: &str) -> Result<FeedData, DomainError>;
}

#[async_trait]
pub trait FeedSourceKeyUpdater: Send + Sync {
    async fn update_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError>;
}

pub type ResolveFeedSourceFuture =
    Pin<Box<dyn Future<Output = Result<ResolvedFeedSource, DomainError>> + Send>>;
pub type ResolveFeedSource = dyn Fn(FeedSource) -> ResolveFeedSourceFuture + Send + Sync;

#[async_trait]
pub trait SearchPoolEventHandler: Send + Sync {
    async fn on_search_started(&self, anime_id: domain::anime::AnimeId);
    async fn on_entry_succeeded(&self, anime_id: domain::anime::AnimeId, feed_data: FeedData);
    async fn on_entry_failed(&self, anime_id: domain::anime::AnimeId);
}
