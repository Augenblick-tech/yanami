use async_trait::async_trait;

use crate::{
    anime::{AnimeId, AnimeMetadata},
    shared::error::DomainError,
};

#[async_trait]
pub trait AnimeLockCap: Send + Sync {
    async fn write_lock_status(&self, anime_id: AnimeId, locked: bool) -> Result<(), DomainError>;
}

#[async_trait]
pub trait AnimeMetadataUpdateCap: Send + Sync {
    async fn update_metadata(
        &self,
        anime_id: AnimeId,
        metadata: &AnimeMetadata,
    ) -> Result<(), DomainError>;
}
