use async_trait::async_trait;
use domain::anime::AnimeId;

#[async_trait]
pub trait AnimeUpdatedHandler: Send + Sync {
    async fn on_anime_updated(&self, anime_id: AnimeId);
}
