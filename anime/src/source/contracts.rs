use std::{future::Future, pin::Pin};

use async_trait::async_trait;

use crate::gateway::{TmdbAlternativeTitles, TmdbSearchResult, TmdbSeriesDetails};
use domain::{
    anime::{AnimeId, AnimeMetadata},
    shared::error::DomainError,
};

/// anime crate 内部使用的季度候选条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeMetadataSeed {
    pub id: i64,
    pub name: String,
    pub weekday: i64,
    pub eps: Option<i64>,
    pub air_date: String,
}

#[async_trait]
pub trait AnimeSource: Send + Sync {
    fn name(&self) -> &str;
    async fn sync(&self) -> Result<Vec<AnimeMetadata>, DomainError>;
}

pub type AnimeSourceFactory =
    dyn Fn() -> Result<Vec<Box<dyn AnimeSource>>, DomainError> + Send + Sync;

#[async_trait]
pub trait SingleAnimeSource: Send + Sync {
    async fn fetch_metadata(&self, bgm_id: AnimeId) -> Result<AnimeMetadata, DomainError>;
}

pub type SearchTmdbTvFuture =
    Pin<Box<dyn Future<Output = Result<TmdbSearchResult, DomainError>> + Send>>;
pub type SearchTmdbTv = dyn Fn(String, String) -> SearchTmdbTvFuture + Send + Sync;

pub type LoadTmdbSeriesDetailsFuture =
    Pin<Box<dyn Future<Output = Result<TmdbSeriesDetails, DomainError>> + Send>>;
pub type LoadTmdbSeriesDetails = dyn Fn(i64, String) -> LoadTmdbSeriesDetailsFuture + Send + Sync;

pub type LoadTmdbAlternativeTitlesFuture =
    Pin<Box<dyn Future<Output = Result<TmdbAlternativeTitles, DomainError>> + Send>>;
pub type LoadTmdbAlternativeTitles = dyn Fn(i64) -> LoadTmdbAlternativeTitlesFuture + Send + Sync;
