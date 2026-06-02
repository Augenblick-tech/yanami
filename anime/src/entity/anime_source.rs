use std::sync::Arc;

use domain::{anime::AnimeMetadata, shared::error::DomainError};

use crate::entity::caps::{AnimeLookupProvider, AnimeSeasonalProvider};

#[derive(Clone)]
pub struct AnimeSources {
    anime_provider: Arc<dyn AnimeLookupProvider>,
    seasonl_providers: Vec<Arc<dyn AnimeSeasonalProvider>>,
}

impl AnimeSources {
    pub fn new(
        searcher: Arc<dyn AnimeLookupProvider>,
        sources: Vec<Arc<dyn AnimeSeasonalProvider>>,
    ) -> Self {
        Self {
            anime_provider: searcher,
            seasonl_providers: sources,
        }
    }

    pub async fn search(&self, keyword: &str) -> Result<Vec<AnimeMetadata>, DomainError> {
        self.anime_provider.search(keyword).await
    }

    pub async fn lookup_by_id(&self, id: u32) -> Result<Option<AnimeMetadata>, DomainError> {
        self.anime_provider.lookup(id).await
    }

    pub async fn sync(&self) -> Result<Vec<AnimeMetadata>, DomainError> {
        let mut metadata: Vec<AnimeMetadata> = vec![];
        for provider in &self.seasonl_providers {
            let data = provider.get().await?;
            for i in data {
                if !metadata.iter().any(|existing| existing.id == i.id) {
                    metadata.push(i);
                }
            }
        }
        Ok(metadata)
    }
}
