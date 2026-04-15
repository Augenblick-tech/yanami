use std::{future::Future, pin::Pin, sync::Arc};

use anime::gateway::{
    BangumiAnimeSearchResultPage, BangumiCalendarAnime, BangumiSubject, BangumiSubjectSmall,
    YucAnimeEntry, YucSeasonCalendarPage,
};
use anime::source::AnimeMetadataSeed;
use chrono::{Datelike, NaiveDate};

use domain::shared::error::DomainError;
use tracing::debug;

use crate::{bangumi::BangumiClient, yuc::YucClient};

type LoadBangumiCalendarAnimeFuture =
    Pin<Box<dyn Future<Output = Result<Vec<BangumiCalendarAnime>, DomainError>> + Send>>;
type LoadBangumiCalendarAnime = dyn Fn() -> LoadBangumiCalendarAnimeFuture + Send + Sync;

type SearchBangumiAnimeFuture =
    Pin<Box<dyn Future<Output = Result<BangumiAnimeSearchResultPage, DomainError>> + Send>>;
type SearchBangumiAnime = dyn Fn(String, u32, u32) -> SearchBangumiAnimeFuture + Send + Sync;

type LoadBangumiSubjectFuture =
    Pin<Box<dyn Future<Output = Result<Option<BangumiSubject>, DomainError>> + Send>>;
type LoadBangumiSubject = dyn Fn(i64) -> LoadBangumiSubjectFuture + Send + Sync;

type LoadYucCurrentSeasonPageFuture =
    Pin<Box<dyn Future<Output = Result<YucSeasonCalendarPage, DomainError>> + Send>>;
type LoadYucCurrentSeasonPage = dyn Fn() -> LoadYucCurrentSeasonPageFuture + Send + Sync;

type LoadYucSeasonPageFuture =
    Pin<Box<dyn Future<Output = Result<YucSeasonCalendarPage, DomainError>> + Send>>;
type LoadYucSeasonPage = dyn Fn(i32, u32) -> LoadYucSeasonPageFuture + Send + Sync;

pub struct BangumiSeedSource {
    load_calendar_anime: Arc<LoadBangumiCalendarAnime>,
}

impl BangumiSeedSource {
    pub fn new(bangumi: BangumiClient) -> Self {
        let bangumi = Arc::new(bangumi);
        Self {
            load_calendar_anime: Arc::new(move || {
                let bangumi = bangumi.clone();
                Box::pin(async move { bangumi.get_calendar_anime().await })
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_loader<F, Fut>(load_calendar_anime: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<BangumiCalendarAnime>, DomainError>> + Send + 'static,
    {
        Self {
            load_calendar_anime: Arc::new(move || Box::pin(load_calendar_anime())),
        }
    }

    pub async fn fetch_anime_metadata_seeds(&self) -> Result<Vec<AnimeMetadataSeed>, DomainError> {
        let items = (self.load_calendar_anime)().await?;
        Ok(items
            .into_iter()
            .map(|item| AnimeMetadataSeed {
                id: item.id,
                name: item.name,
                weekday: item.weekday,
                eps: item.eps,
                air_date: item.air_date,
            })
            .collect())
    }

    pub async fn fetch_season_anime_metadata_seeds(
        &self,
        year: i32,
        month: u32,
    ) -> Result<Vec<AnimeMetadataSeed>, DomainError> {
        let items = (self.load_calendar_anime)().await?;
        Ok(items
            .into_iter()
            .filter(|item| matches_requested_season(&item.air_date, year, month))
            .map(|item| AnimeMetadataSeed {
                id: item.id,
                name: item.name,
                weekday: item.weekday,
                eps: item.eps,
                air_date: item.air_date,
            })
            .collect())
    }
}

pub struct YucBangumiSeedSource {
    load_current_season_page: Arc<LoadYucCurrentSeasonPage>,
    load_season_page: Arc<LoadYucSeasonPage>,
    search_bangumi_anime: Arc<SearchBangumiAnime>,
    load_bangumi_subject: Arc<LoadBangumiSubject>,
}

impl YucBangumiSeedSource {
    pub fn new(yuc: YucClient, bangumi: BangumiClient) -> Self {
        let yuc = Arc::new(yuc);
        let bangumi = Arc::new(bangumi);
        Self {
            load_current_season_page: Arc::new({
                let yuc = yuc.clone();
                move || {
                    let yuc = yuc.clone();
                    Box::pin(async move { yuc.fetch_current_season_calendar().await })
                }
            }),
            load_season_page: Arc::new({
                let yuc = yuc.clone();
                move |year, month| {
                    let yuc = yuc.clone();
                    Box::pin(async move { yuc.fetch_season_calendar(year, month).await })
                }
            }),
            search_bangumi_anime: Arc::new({
                let bangumi = bangumi.clone();
                move |keyword, limit, offset| {
                    let bangumi = bangumi.clone();
                    Box::pin(async move { bangumi.search_anime(&keyword, limit, offset).await })
                }
            }),
            load_bangumi_subject: Arc::new(move |id| {
                let bangumi = bangumi.clone();
                Box::pin(async move { bangumi.get_subject(id).await })
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_loaders<FC, FCut, FS, FSut, FB, FBut, FSu, FSut2>(
        load_current_season_page: FC,
        load_season_page: FS,
        search_bangumi_anime: FB,
        load_bangumi_subject: FSu,
    ) -> Self
    where
        FC: Fn() -> FCut + Send + Sync + 'static,
        FCut: Future<Output = Result<YucSeasonCalendarPage, DomainError>> + Send + 'static,
        FS: Fn(i32, u32) -> FSut + Send + Sync + 'static,
        FSut: Future<Output = Result<YucSeasonCalendarPage, DomainError>> + Send + 'static,
        FB: Fn(String, u32, u32) -> FBut + Send + Sync + 'static,
        FBut: Future<Output = Result<BangumiAnimeSearchResultPage, DomainError>> + Send + 'static,
        FSu: Fn(i64) -> FSut2 + Send + Sync + 'static,
        FSut2: Future<Output = Result<Option<BangumiSubject>, DomainError>> + Send + 'static,
    {
        Self {
            load_current_season_page: Arc::new(move || Box::pin(load_current_season_page())),
            load_season_page: Arc::new(move |year, month| Box::pin(load_season_page(year, month))),
            search_bangumi_anime: Arc::new(move |keyword, limit, offset| {
                Box::pin(search_bangumi_anime(keyword, limit, offset))
            }),
            load_bangumi_subject: Arc::new(move |id| Box::pin(load_bangumi_subject(id))),
        }
    }

    pub async fn fetch_anime_metadata_seeds(&self) -> Result<Vec<AnimeMetadataSeed>, DomainError> {
        let page = (self.load_current_season_page)().await?;
        debug!(
            source = "yuc",
            page_url = %page.page_url,
            season_code = %page.season_code,
            entry_count = page.entries.len(),
            "yuc season page loaded"
        );
        map_yuc_page_to_seeds(
            &page,
            self.search_bangumi_anime.as_ref(),
            self.load_bangumi_subject.as_ref(),
        )
        .await
    }

    pub async fn fetch_season_anime_metadata_seeds(
        &self,
        year: i32,
        month: u32,
    ) -> Result<Vec<AnimeMetadataSeed>, DomainError> {
        let page = (self.load_season_page)(year, month).await?;
        debug!(
            source = "yuc",
            page_url = %page.page_url,
            season_code = %page.season_code,
            entry_count = page.entries.len(),
            "yuc season page loaded"
        );
        map_yuc_page_to_seeds(
            &page,
            self.search_bangumi_anime.as_ref(),
            self.load_bangumi_subject.as_ref(),
        )
        .await
    }
}

async fn map_yuc_page_to_seeds(
    page: &YucSeasonCalendarPage,
    search_bangumi_anime: &SearchBangumiAnime,
    load_bangumi_subject: &LoadBangumiSubject,
) -> Result<Vec<AnimeMetadataSeed>, DomainError> {
    let mut seeds = Vec::new();

    for entry in &page.entries {
        let Some(seed) =
            resolve_yuc_entry_to_seed(entry, search_bangumi_anime, load_bangumi_subject).await?
        else {
            tracing::trace!(
                source = "yuc",
                title_zh = %entry.title_zh,
                title_original = ?entry.title_original,
                "yuc entry skipped: no bangumi seed resolved"
            );
            continue;
        };
        seeds.push(seed);
    }

    tracing::debug!(
        source = "yuc",
        page_url = %page.page_url,
        entry_count = page.entries.len(),
        seed_count = seeds.len(),
        "yuc season page mapped to bangumi seeds"
    );

    Ok(seeds)
}

async fn resolve_yuc_entry_to_seed(
    entry: &YucAnimeEntry,
    search_bangumi_anime: &SearchBangumiAnime,
    load_bangumi_subject: &LoadBangumiSubject,
) -> Result<Option<AnimeMetadataSeed>, DomainError> {
    let mut keywords = Vec::new();
    if let Some(title_original) = &entry.title_original {
        keywords.push(title_original.clone());
    }
    keywords.push(entry.title_zh.clone());

    for keyword in keywords {
        let page = search_bangumi_anime(keyword.clone(), 10, 0).await?;
        tracing::trace!(
            source = "yuc",
            title_zh = %entry.title_zh,
            title_original = ?entry.title_original,
            keyword = %keyword,
            candidate_count = page.data.len(),
            "yuc entry searched bangumi"
        );
        let Some(subject) = select_bangumi_subject(entry, &page.data) else {
            tracing::trace!(
                source = "yuc",
                title_zh = %entry.title_zh,
                title_original = ?entry.title_original,
                "yuc entry skipped: bangumi search candidates did not match"
            );
            continue;
        };
        let subject_detail = load_bangumi_subject(subject.id).await?;
        let Some(air_date) = subject_detail
            .as_ref()
            .and_then(|detail| detail.air_date.clone())
            .or_else(|| subject.air_date.clone())
        else {
            tracing::trace!(
                source = "yuc",
                title_zh = %entry.title_zh,
                title_original = ?entry.title_original,
                anime_id = subject.id,
                anime_name = %subject.name,
                "yuc entry skipped: bangumi air date missing"
            );
            continue;
        };
        let Some(weekday) = subject
            .air_weekday
            .or_else(|| weekday_from_air_date(&air_date))
        else {
            tracing::trace!(
                source = "yuc",
                title_zh = %entry.title_zh,
                title_original = ?entry.title_original,
                anime_id = subject.id,
                anime_name = %subject.name,
                air_date = %air_date,
                "yuc entry skipped: bangumi weekday missing and air date invalid"
            );
            continue;
        };
        let Some(eps) = subject_detail
            .as_ref()
            .and_then(|detail| detail.eps)
            .or(subject.eps)
        else {
            tracing::trace!(
                source = "yuc",
                title_zh = %entry.title_zh,
                title_original = ?entry.title_original,
                anime_id = subject.id,
                anime_name = %subject.name,
                "yuc entry skipped: bangumi episode count missing"
            );
            continue;
        };

        return Ok(Some(AnimeMetadataSeed {
            id: subject.id,
            name: subject.name.clone(),
            weekday,
            eps: Some(eps),
            air_date,
        }));
    }

    Ok(None)
}

fn select_bangumi_subject<'a>(
    entry: &YucAnimeEntry,
    subjects: &'a [BangumiSubjectSmall],
) -> Option<&'a BangumiSubjectSmall> {
    let normalized_zh = normalize_title(&entry.title_zh);
    let normalized_original = entry
        .title_original
        .as_ref()
        .map(|title| normalize_title(title));

    subjects
        .iter()
        .find(|subject| {
            normalize_title(&subject.name_cn) == normalized_zh
                || normalize_title(&subject.name) == normalized_zh
                || normalized_original
                    .as_ref()
                    .is_some_and(|title| normalize_title(&subject.name) == *title)
        })
        .or_else(|| {
            subjects.iter().find(|subject| {
                normalized_title_contains(&subject.name_cn, &entry.title_zh)
                    || normalized_title_contains(&subject.name, &entry.title_zh)
                    || entry
                        .title_original
                        .as_ref()
                        .is_some_and(|title| normalized_title_contains(&subject.name, title))
            })
        })
}

fn normalized_title_contains(haystack: &str, needle: &str) -> bool {
    let haystack = normalize_title(haystack);
    let needle = normalize_title(needle);
    !needle.is_empty() && haystack.contains(&needle)
}

fn normalize_title(title: &str) -> String {
    title.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn matches_requested_season(air_date: &str, year: i32, month: u32) -> bool {
    let Ok(date) = NaiveDate::parse_from_str(air_date, "%Y-%m-%d") else {
        return false;
    };

    normalize_season(date.year(), date.month()) == (year, month)
}

fn normalize_season(year: i32, month: u32) -> (i32, u32) {
    match month {
        1..=3 => (year, 1),
        4..=6 => (year, 4),
        7..=9 => (year, 7),
        10..=12 => (year, 10),
        _ => (year, month),
    }
}

fn weekday_from_air_date(air_date: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(air_date, "%Y-%m-%d").ok()?;
    Some(date.weekday().num_days_from_monday() as i64 + 1)
}
