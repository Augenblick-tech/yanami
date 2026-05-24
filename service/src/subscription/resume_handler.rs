use std::sync::Arc;

use anime::animes::Animes;
use anime::contracts::AnimeUpdatedHandler;
use async_trait::async_trait;
use domain::anime::AnimeId;
use subscription::SubscriptionAnimes;

use crate::shared::error::ApplicationError;

pub struct ResumeCompletedSubscriptions {
    animes: Arc<Animes>,
    subscriptions: Arc<SubscriptionAnimes>,
}

impl ResumeCompletedSubscriptions {
    pub fn new(animes: Arc<Animes>, subscriptions: Arc<SubscriptionAnimes>) -> Self {
        Self {
            animes,
            subscriptions,
        }
    }
}

#[async_trait]
impl AnimeUpdatedHandler for ResumeCompletedSubscriptions {
    async fn on_anime_updated(&self, anime_id: AnimeId) {
        let result = self.try_resume(anime_id).await;
        if let Err(error) = result {
            tracing::error!(?anime_id, ?error, "resume_completed_subscriptions failed");
        }
    }
}

impl ResumeCompletedSubscriptions {
    async fn try_resume(&self, anime_id: AnimeId) -> Result<(), ApplicationError> {
        let anime = self.animes.load(anime_id).await?;
        let planned = anime.read_data().metadata.planned_episode_count.0;
        let subscriptions = self.subscriptions.list_by_anime(anime_id).await?;
        for mut entity in subscriptions {
            if entity
                .resume_if_completed(planned, &*self.subscriptions.caps.toggle)
                .await?
            {}
        }
        Ok(())
    }
}
