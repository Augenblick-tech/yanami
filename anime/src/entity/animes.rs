use std::sync::Arc;

use domain::{
    anime::{AnimeId, AnimeListQuery, AnimeMetadata},
    shared::error::DomainError,
};

use crate::entity::{anime_entity::AnimeEntity, caps::AnimeRepository};

pub struct Animes {
    repo: Arc<dyn AnimeRepository>,
}

impl Animes {
    pub fn new(repo: Arc<dyn AnimeRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeEntity>, DomainError> {
        let list = self.repo.list(query).await?;
        Ok(list)
    }

    pub async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeEntity>, DomainError> {
        let entity = self.repo.find(anime_id).await?;
        Ok(entity)
    }

    pub async fn create(&self, metadata: AnimeMetadata) -> Result<AnimeEntity, DomainError> {
        if self.repo.exist(metadata.id).await? {
            return Err(DomainError::InvariantViolation("anime was exist"));
        }
        let entity = AnimeEntity::new(metadata, false);
        self.repo.insert(&entity).await?;
        Ok(entity)
    }

    pub async fn save(&self, entity: &AnimeEntity) -> Result<(), DomainError> {
        self.repo.update(entity).await
    }

    pub async fn sync_metadata(&self, metadata: Vec<AnimeMetadata>) -> Result<(), DomainError> {
        if metadata.is_empty() {
            return Ok(());
        }
        self.repo.sync_metadata_with_not_lock(metadata).await
    }
}
