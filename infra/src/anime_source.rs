use std::sync::Arc;

use anime::source::build_anime_metadata;
use anime::source::{AnimeMetadataSeed, AnimeSource, SingleAnimeSource};
use async_trait::async_trait;
use chrono::Datelike;
use domain::{
    anime::{AnimeId, AnimeMetadata},
    shared::error::DomainError,
};

use crate::bangumi::BangumiClient;
use crate::seed_source::YucBangumiSeedSource;
use crate::tmdb::TmdbClient;
use crate::yuc::YucClient;

pub struct BangumiSource {
    bangumi: Arc<BangumiClient>,
    tmdb: Arc<TmdbClient>,
}

impl BangumiSource {
    pub fn new(bangumi: BangumiClient, tmdb: TmdbClient) -> Self {
        Self {
            bangumi: Arc::new(bangumi),
            tmdb: Arc::new(tmdb),
        }
    }
}

#[async_trait]
impl AnimeSource for BangumiSource {
    fn name(&self) -> &str {
        "bangumi"
    }

    async fn sync(&self) -> Result<Vec<AnimeMetadata>, DomainError> {
        let items = self.bangumi.get_calendar_anime().await?;
        for item in &items {
            tracing::trace!(
                source = self.name(),
                anime_id = item.id,
                anime_name = %item.name,
                weekday = item.weekday,
                eps = ?item.eps,
                air_date = %item.air_date,
                "anime source loaded seed"
            );
        }
        let seeds: Vec<AnimeMetadataSeed> = items
            .into_iter()
            .map(|item| AnimeMetadataSeed {
                id: item.id,
                name: item.name,
                weekday: item.weekday,
                eps: item.eps,
                air_date: item.air_date,
            })
            .collect();
        let tmdb = self.tmdb.clone();
        build_anime_metadata(
            &seeds,
            &{
                let tmdb = tmdb.clone();
                move |query, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.search_tv(&query, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_series_details(series_id, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_alternative_titles(series_id).await })
                }
            },
        )
        .await
    }
}

pub struct YucSource {
    seed_source: YucBangumiSeedSource,
    tmdb: Arc<TmdbClient>,
}

impl YucSource {
    pub fn new(yuc: YucClient, bangumi: BangumiClient, tmdb: TmdbClient) -> Self {
        Self {
            seed_source: YucBangumiSeedSource::new(yuc, bangumi),
            tmdb: Arc::new(tmdb),
        }
    }
}

#[async_trait]
impl AnimeSource for YucSource {
    fn name(&self) -> &str {
        "yuc"
    }

    async fn sync(&self) -> Result<Vec<AnimeMetadata>, DomainError> {
        let seeds = self.seed_source.fetch_anime_metadata_seeds().await?;
        for seed in &seeds {
            tracing::trace!(
                source = self.name(),
                anime_id = seed.id,
                anime_name = %seed.name,
                weekday = seed.weekday,
                eps = ?seed.eps,
                air_date = %seed.air_date,
                "anime source loaded seed"
            );
        }
        let tmdb = self.tmdb.clone();
        build_anime_metadata(
            &seeds,
            &{
                let tmdb = tmdb.clone();
                move |query, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.search_tv(&query, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_series_details(series_id, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_alternative_titles(series_id).await })
                }
            },
        )
        .await
    }
}

pub struct BangumiSingleSource {
    bangumi: Arc<BangumiClient>,
    tmdb: Arc<TmdbClient>,
}

impl BangumiSingleSource {
    pub fn new(bangumi: BangumiClient, tmdb: TmdbClient) -> Self {
        Self {
            bangumi: Arc::new(bangumi),
            tmdb: Arc::new(tmdb),
        }
    }
}

#[async_trait]
impl SingleAnimeSource for BangumiSingleSource {
    async fn fetch_metadata(&self, bgm_id: AnimeId) -> Result<AnimeMetadata, DomainError> {
        let subject = self
            .bangumi
            .get_subject(bgm_id.0)
            .await?
            .ok_or(DomainError::InvariantViolation("bgm subject not found"))?;
        let name = subject.name.ok_or(DomainError::InvariantViolation(
            "bgm subject name is missing",
        ))?;
        let air_date = subject.air_date.ok_or(DomainError::InvariantViolation(
            "bgm subject air_date is missing",
        ))?;

        let date = chrono::NaiveDate::parse_from_str(&air_date, "%Y-%m-%d").map_err(|error| {
            DomainError::external("bgm subject air_date is not yyyy-mm-dd", error)
        })?;
        let weekday = date.weekday().num_days_from_sunday() as i64;

        let seed = AnimeMetadataSeed {
            id: bgm_id.0,
            name,
            weekday,
            eps: subject.eps,
            air_date,
        };
        let tmdb = self.tmdb.clone();
        let mut results = build_anime_metadata(
            &[seed],
            &{
                let tmdb = tmdb.clone();
                move |query, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.search_tv(&query, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id, language| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_series_details(series_id, &language).await })
                }
            },
            &{
                let tmdb = tmdb.clone();
                move |series_id| {
                    let tmdb = tmdb.clone();
                    Box::pin(async move { tmdb.get_alternative_titles(series_id).await })
                }
            },
        )
        .await?;
        results.pop().ok_or(DomainError::InvariantViolation(
            "build_anime_metadata returned no results for bgm_id",
        ))
    }
}
