use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{
    cap::SubscribedAnimeEpisodeCaps, subscribed_anime_episode_entity::SubscribedAnimeEpisodeEntity,
};

#[derive(Clone)]
pub struct SubscribedAnimeEpisodes {
    sub_anime_id: u32,
    repo: Arc<dyn SubscribedAnimeEpisodeCaps>,
}

impl SubscribedAnimeEpisodes {
    pub(crate) fn new(sub_anime_id: u32, repo: Arc<dyn SubscribedAnimeEpisodeCaps>) -> Self {
        Self { sub_anime_id, repo }
    }

    pub async fn list(&self) -> Result<Vec<SubscribedAnimeEpisodeEntity>, DomainError> {
        self.repo.list_eps(self.sub_anime_id).await
    }

    pub async fn create(&self, sub_anime_id: u32, resource_id: u32) -> Result<SubscribedAnimeEpisodeEntity, DomainError> {
        self.repo.add_epsiode(sub_anime_id, resource_id).await
    }
}
