use async_trait::async_trait;

use domain::{
    anime::{AnimeId, AnimeListQuery, AnimeMetadata},
    shared::error::DomainError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeSnapshot {
    pub metadata: AnimeMetadata,
    pub metadata_locked: bool,
}

#[async_trait]
pub trait AnimeRepository: Send + Sync {
    async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeSnapshot>, DomainError>;

    async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeSnapshot>, DomainError>;

    async fn list_by_ids(&self, anime_ids: &[AnimeId]) -> Result<Vec<AnimeSnapshot>, DomainError>;
}
