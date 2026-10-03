use std::ops::ControlFlow;

use anyhow::Result;
use async_trait::async_trait;

use crate::entity::model::{
    AnimeBaseData, AnimeListQuery, AnimeMetadata, AnimeProps, AnimeSearchResult,
    AnimeSeriesMetadata,
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

    /// 写入一部新番剧：番剧本体、标题、外部链接、季度与系列信息在同一个事务里落库，
    /// 锁状态随创建一起生效，避免「先建后改」的两次写入。
    async fn insert(&self, entity: &AnimeMetadata, lock: bool) -> Result<AnimeProps>;

    async fn update(&self, entity: &AnimeBaseData) -> Result<()>;

    async fn set_lock(&self, anime_id: i64, lock: bool) -> Result<()>;

    async fn sync_metadata_with_not_lock(
        &self,
        metadata: Vec<AnimeMetadata>,
    ) -> Result<Vec<AnimeProps>>;

    /// 取回一个系列的展示信息，一个剧集只有一份。
    async fn find_series(&self, tmdb_id: i64) -> Result<Option<AnimeSeriesMetadata>>;

    /// 取回一个系列的全部季度条目，顺序不做要求。
    async fn list_by_series(&self, tmdb_id: i64) -> Result<Vec<AnimeProps>>;
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
