use std::{ops::ControlFlow, sync::Arc};

use common::shared::error::Error;

use crate::entity::{
    anime_entity::AnimeEntity,
    cap::AnimeRepository,
    model::{AnimeBaseData, AnimeListQuery, AnimeMetadata, AnimeProps},
};

#[derive(Clone)]
pub struct Animes {
    repo: Arc<dyn AnimeRepository>,
}

impl Animes {
    pub fn new(repo: Arc<dyn AnimeRepository>) -> Self {
        Self { repo }
    }

    pub async fn list_by_ids(&self, ids: Vec<i64>) -> Result<Vec<AnimeEntity>, Error> {
        let props = self
            .repo
            .list_by_ids(&ids)
            .await
            .map_err(|e| Error::external("animes get list by anime ids failed", e))?;
        Ok(props
            .into_iter()
            .map(|i| AnimeEntity::new(i.data))
            .collect())
    }

    pub async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeEntity>, Error> {
        let list = self
            .repo
            .list(&query)
            .await
            .map_err(|e| Error::external("animes list anime_entity failed", e))?;
        Ok(list.into_iter().map(|i| AnimeEntity::new(i.data)).collect())
    }

    pub async fn range<F>(&self, query: &AnimeListQuery, mut f: F) -> Result<(), Error>
    where
        F: FnMut(AnimeEntity) -> anyhow::Result<ControlFlow<()>> + Send + 'static,
    {
        self.repo
            .range(query, &mut |props: AnimeProps| {
                f(AnimeEntity::new(props.data))
            })
            .await
            .map_err(|e| Error::external("animes range anime_entity failed", e))
    }

    pub async fn get(&self, anime_id: i64) -> Result<Option<AnimeEntity>, Error> {
        let props = self
            .repo
            .find(anime_id)
            .await
            .map_err(|e| Error::external("anims get anime_entity failed", e))?;
        if let Some(props) = props {
            Ok(Some(AnimeEntity::new(props.data)))
        } else {
            Ok(None)
        }
    }

    pub async fn create(&self, metadata: AnimeMetadata) -> Result<AnimeEntity, Error> {
        let props = self
            .repo
            .insert(&metadata)
            .await
            .map_err(|e| Error::external("animes create anime_entity failed", e))?;
        Ok(AnimeEntity::new(props.data))
    }

    pub async fn save(&self, entity: &AnimeEntity) -> Result<(), Error> {
        self.repo
            .update(&AnimeBaseData {
                id: entity.id(),
                metadata: entity.metadata().clone(),
                lock: entity.is_locked(),
            })
            .await
            .map_err(|e| Error::external("animes save anime_entity failed", e))
    }

    pub async fn sync_metadata(
        &self,
        metadata: Vec<AnimeMetadata>,
    ) -> Result<Vec<AnimeEntity>, Error> {
        if metadata.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .repo
            .sync_metadata_with_not_lock(metadata)
            .await
            .map_err(|e| Error::external("animes sync anime metadata failed", e))?
            .into_iter()
            .map(|i| AnimeEntity::new(i.data))
            .collect())
    }
}
