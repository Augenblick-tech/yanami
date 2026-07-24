use std::ops::ControlFlow;

use anyhow::Result;
use async_trait::async_trait;

use crate::entity::model::{
    AnimeBaseData, AnimeListQuery, AnimeMetadata, AnimeProps, AnimeSearchResult,
};

pub trait AnimeConsumer: Send {
    fn consume(&mut self, data: AnimeProps) -> Result<ControlFlow<()>>;
}

impl<F> AnimeConsumer for F
where
    F: FnMut(AnimeProps) -> Result<ControlFlow<()>> + Send,
{
    fn consume(&mut self, anime: AnimeProps) -> Result<ControlFlow<()>> {
        (self)(anime)
    }
}

#[async_trait]
pub trait AnimeRepository: Send + Sync {
    async fn list(&self, query: &AnimeListQuery) -> Result<Vec<AnimeProps>>;

    async fn range(&self, query: &AnimeListQuery, consumer: &mut dyn AnimeConsumer) -> Result<()>;

    async fn find(&self, anime_id: i64) -> Result<Option<AnimeProps>>;

    async fn list_by_ids(&self, anime_ids: &[i64]) -> Result<Vec<AnimeProps>>;

    async fn insert(&self, entity: &AnimeMetadata) -> Result<AnimeProps>;

    async fn update(&self, entity: &AnimeBaseData) -> Result<()>;

    async fn set_lock(&self, anime_id: i64, lock: bool) -> Result<()>;

    async fn sync_metadata_with_not_lock(&self, metadata: Vec<AnimeMetadata>) -> Result<Vec<AnimeProps>>;
}

#[async_trait]
pub trait AnimeSeasonalProvider: Send + Sync {
    async fn get(&self) -> Result<Vec<AnimeMetadata>>;
    fn name(&self) -> &str;
}

#[async_trait]
pub trait AnimeLookupProvider: Send + Sync {
    async fn search(&self, keyword: &str) -> Result<Vec<AnimeSearchResult>>;
    async fn lookup(&self, id: i64) -> Result<Option<AnimeMetadata>>;
}
