use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{cap::SubscribedAnimeRepository, subscribed_anime_entity::SubscribedAnimeEntity, subscribed_anime_episodes::SubscribedAnimeEpisodes};


#[derive(Clone)]
pub struct SubscribedAnimes {
    repo: Arc<dyn SubscribedAnimeRepository>,
}

impl SubscribedAnimes {
    pub fn new(repo: Arc<dyn SubscribedAnimeRepository>) -> Self {
        Self { repo }
    }

    pub async fn find(
        &self,
        sub_anime_id: u32,
    ) -> Result<Option<SubscribedAnimeEntity>, DomainError> {
        Ok(self.repo.find(sub_anime_id).await?)
    }

    pub async fn episodes_of(&self, entity: &SubscribedAnimeEntity) -> SubscribedAnimeEpisodes {
        SubscribedAnimeEpisodes::new(entity.id(), self.repo.clone())
    }
}
