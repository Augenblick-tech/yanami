use async_trait::async_trait;
use domain::{
    anime::{AnimeId, AnimeListQuery, AnimeMetadata},
    shared::error::DomainError,
};

use crate::entity::anime_entity::AnimeEntity;

#[async_trait]
pub trait AnimeRepository: Send + Sync {
    async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeEntity>, DomainError>;

    async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeEntity>, DomainError>;

    async fn list_by_ids(&self, anime_ids: &[AnimeId]) -> Result<Vec<AnimeEntity>, DomainError>;

    async fn insert(&self, entity: &AnimeEntity) -> Result<(), DomainError>;

    async fn update(&self, entity: &AnimeEntity) -> Result<(), DomainError>;

    async fn exist(&self, anime_id: AnimeId) -> Result<bool, DomainError>;

    async fn sync_metadata_with_not_lock(
        &self,
        metadata: Vec<AnimeMetadata>,
    ) -> Result<(), DomainError>;
}

#[async_trait]
pub trait AnimeSeasonalProvider: Send + Sync {
    async fn get(&self) -> Result<Vec<AnimeMetadata>, DomainError>;
}

#[async_trait]
pub trait AnimeLookupProvider: Send + Sync {
    async fn search(&self, keyword: &str) -> Result<Vec<AnimeMetadata>, DomainError>;
    async fn lookup(&self, id: u32) -> Result<Option<AnimeMetadata>, DomainError>;
}
