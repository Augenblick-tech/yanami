use std::sync::Arc;

use common::shared::error::Error;

use crate::entity::{
    cap::{AnimeLookupProvider, AnimeSeasonalProvider},
    model::{AnimeMetadata, AnimeSearchResult, AnimeSourceTarget},
};

#[derive(Clone)]
pub struct AnimeSources {
    anime_provider: Arc<dyn AnimeLookupProvider>,
    seasonal_providers: Vec<Arc<dyn AnimeSeasonalProvider>>,
}

impl AnimeSources {
    pub fn new(
        searcher: Arc<dyn AnimeLookupProvider>,
        sources: Vec<Arc<dyn AnimeSeasonalProvider>>,
    ) -> Self {
        Self {
            anime_provider: searcher,
            seasonal_providers: sources,
        }
    }

    pub async fn search(&self, keyword: &str) -> Result<Vec<AnimeSearchResult>, Error> {
        self.anime_provider
            .search(keyword)
            .await
            .map_err(|e| Error::external("anime source search failed", e))
    }

    pub async fn lookup_by_id(&self, id: i64) -> Result<Option<AnimeMetadata>, Error> {
        self.anime_provider
            .lookup(id)
            .await
            .map_err(|e| Error::external("anime source lookup failed", e))
    }

    pub async fn sync(&self) -> Result<Vec<AnimeMetadata>, Error> {
        let mut metadata: Vec<AnimeMetadata> = vec![];
        for provider in &self.seasonal_providers {
            match provider.get().await {
                Ok(data) => {
                    for i in data {
                        if !metadata.iter().any(|existing| {
                            for y in &i.external_link {
                                let AnimeSourceTarget::Bangumi = y.target else {
                                    continue;
                                };
                                for x in &existing.external_link {
                                    let AnimeSourceTarget::Bangumi = x.target else {
                                        continue;
                                    };
                                    if y.id == x.id {
                                        return true;
                                    }
                                }
                            }

                            false
                        }) {
                            metadata.push(i);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "anime source sync provider {} failed, error = {e}",
                        provider.name()
                    );
                }
            }
        }
        Ok(metadata)
    }
}
