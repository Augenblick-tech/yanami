use std::sync::Arc;

use domain::{
    anime::AnimeId,
    feed::FeedSourceId,
    shared::error::DomainError,
    subscription::{SearchPoolEntry, SearchPoolRepository},
};

#[derive(Clone)]
pub struct SearchPool {
    repository: Arc<dyn SearchPoolRepository>,
}

impl SearchPool {
    pub fn new(repository: Arc<dyn SearchPoolRepository>) -> Self {
        Self { repository }
    }

    pub async fn list_distinct_feed_ids(&self) -> Result<Vec<FeedSourceId>, DomainError> {
        self.repository.list_distinct_feed_ids().await
    }

    pub async fn pick_random(
        &self,
        feed_id: &FeedSourceId,
    ) -> Result<Option<SearchPoolEntry>, DomainError> {
        self.repository.pick_random(feed_id).await
    }

    /// 消费一条池条目：删除关联链接和条目自身。调用方应先 pick_random 获取条目。
    pub async fn consume_entry(&self, entry: &SearchPoolEntry) -> Result<(), DomainError> {
        self.repository.delete_sub_links_by_pool(entry.id).await?;
        self.repository.delete_entry(entry.id).await
    }

    pub async fn count_by_anime(&self, anime_id: AnimeId) -> Result<i64, DomainError> {
        self.repository.count_by_anime(anime_id).await
    }
}
