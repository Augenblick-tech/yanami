use std::sync::Arc;

use domain::{anime::AnimeId, subscription::SubscriptionSearchState};
use feed::contracts::{FeedData, SearchPoolEventHandler};
use subscription::entity::SubscriptionAnimeEntity;
use subscription::search_pool::SearchPool;

use crate::subscription::service::SubscriptionService;

pub struct SearchPoolEventProcessor {
    subscription_service: Arc<SubscriptionService>,
    search_pool: Arc<SearchPool>,
}

impl SearchPoolEventProcessor {
    pub fn new(
        subscription_service: Arc<SubscriptionService>,
        search_pool: Arc<SearchPool>,
    ) -> Self {
        Self {
            subscription_service,
            search_pool,
        }
    }
}

#[async_trait::async_trait]
impl SearchPoolEventHandler for SearchPoolEventProcessor {
    async fn on_search_started(&self, anime_id: AnimeId) {
        if let Err(error) = self
            .subscription_service
            .batch_update_search_state_by_anime(anime_id, SubscriptionSearchState::Running)
            .await
        {
            tracing::error!(
                ?anime_id,
                ?error,
                "pool_handler: mark search running failed"
            );
        }
    }

    async fn on_entry_succeeded(&self, anime_id: AnimeId, feed_data: FeedData) {
        if let Err(error) = self
            .subscription_service
            .found_pool_resources(feed_data, anime_id)
            .await
        {
            tracing::error!(
                ?anime_id,
                ?error,
                "pool_handler: found_pool_resources failed"
            );
        }
        let remaining = match self.search_pool.count_by_anime(anime_id).await {
            Ok(count) => count,
            Err(error) => {
                tracing::error!(?anime_id, ?error, "pool_handler: count_by_anime failed");
                return;
            }
        };
        let target_state = SubscriptionAnimeEntity::decide_search_target_state(remaining);
        if let Err(error) = self
            .subscription_service
            .batch_update_search_state_by_anime(anime_id, target_state)
            .await
        {
            tracing::error!(
                ?anime_id,
                ?error,
                "pool_handler: update search state failed"
            );
        }
    }

    async fn on_entry_failed(&self, anime_id: AnimeId) {
        let remaining = match self.search_pool.count_by_anime(anime_id).await {
            Ok(count) => count,
            Err(error) => {
                tracing::error!(?anime_id, ?error, "pool_handler: count_by_anime failed");
                return;
            }
        };
        let target_state = SubscriptionAnimeEntity::decide_search_target_state(remaining);
        if let Err(error) = self
            .subscription_service
            .batch_update_search_state_by_anime(anime_id, target_state)
            .await
        {
            tracing::error!(
                ?anime_id,
                ?error,
                "pool_handler: update search state failed"
            );
        }
    }
}
