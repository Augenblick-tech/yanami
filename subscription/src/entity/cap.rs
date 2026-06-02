use async_trait::async_trait;
use domain::shared::error::DomainError;

use crate::entity::{
    subscribed_anime_entity::SubscribedAnimeEntity,
    subscribed_anime_episode_entity::SubscribedAnimeEpisodeEntity,
};

#[async_trait]
pub trait SubscribedAnimeRepository: Send + Sync + SubscribedAnimeEpisodeCaps {
    async fn find(&self, sub_anime_id: u32) -> Result<Option<SubscribedAnimeEntity>, DomainError>;
}

#[async_trait]
pub trait SubscribedAnimeEpisodeCaps: Send + Sync {
    async fn list_eps(
        &self,
        sub_anime_id: u32,
    ) -> Result<Vec<SubscribedAnimeEpisodeEntity>, DomainError>;

    async fn add_epsiode(&self, sub_anime_id: u32, resource_id: u32) -> Result<SubscribedAnimeEpisodeEntity, DomainError>;
}
