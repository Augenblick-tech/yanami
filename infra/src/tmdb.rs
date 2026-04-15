use std::time::Duration;

use anime::gateway::{
    TmdbAlternativeTitleItem, TmdbAlternativeTitles, TmdbSearchResult, TmdbSearchResultItem,
    TmdbSeason, TmdbSeriesDetails,
};
use anyhow::{Context, Error};
use domain::shared::error::DomainError;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct TmdbClient {
    client: Client,
    base_url: String,
}

impl TmdbClient {
    pub fn new(key: &str) -> Result<Self, Error> {
        Self::with_base_url(key, "https://api.themoviedb.org/3")
    }

    pub fn with_base_url(key: &str, base_url: &str) -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(format!("Bearer {key}").as_str()).context("invalid tmdb key")?,
        );
        headers.insert(header::ACCEPT, "application/json".parse()?);
        Ok(Self {
            client: Client::builder()
                .default_headers(headers)
                .timeout(Duration::from_secs(30))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn search_tv(
        &self,
        query: &str,
        language: &str,
    ) -> Result<TmdbSearchResult, DomainError> {
        let response = self
            .client
            .get(format!(
                "{}/search/tv?query={query}&include_adult=true&language={language}",
                self.base_url
            ))
            .send()
            .await
            .map_err(|error| DomainError::external("tmdb search request failed", error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| DomainError::external("tmdb search body read failed", error))?;
        let payload: TmdbSearchResultPayload = serde_json::from_str(&body).map_err(|_error| {
            DomainError::external(
                "tmdb search decode failed",
                anyhow::anyhow!(
                    "status={status}, body={}",
                    &body.chars().take(512).collect::<String>()
                ),
            )
        })?;
        Ok(payload.into())
    }

    pub async fn get_series_details(
        &self,
        series_id: i64,
        language: &str,
    ) -> Result<TmdbSeriesDetails, DomainError> {
        let response = self
            .client
            .get(format!(
                "{}/tv/{series_id}?language={language}",
                self.base_url
            ))
            .send()
            .await
            .map_err(|error| DomainError::external("tmdb series request failed", error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| DomainError::external("tmdb series body read failed", error))?;
        let payload: TmdbSeriesDetailsPayload = serde_json::from_str(&body).map_err(|_error| {
            DomainError::external(
                "tmdb series decode failed",
                anyhow::anyhow!(
                    "status={status}, body={}",
                    &body.chars().take(512).collect::<String>()
                ),
            )
        })?;
        Ok(payload.into())
    }

    pub async fn get_alternative_titles(
        &self,
        series_id: i64,
    ) -> Result<TmdbAlternativeTitles, DomainError> {
        let response = self
            .client
            .get(format!(
                "{}/tv/{series_id}/alternative_titles",
                self.base_url
            ))
            .send()
            .await
            .map_err(|error| DomainError::external("tmdb titles request failed", error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| DomainError::external("tmdb titles body read failed", error))?;
        let payload: TmdbAlternativeTitlesPayload =
            serde_json::from_str(&body).map_err(|_error| {
                DomainError::external(
                    "tmdb titles decode failed",
                    anyhow::anyhow!(
                        "status={status}, body={}",
                        &body.chars().take(512).collect::<String>()
                    ),
                )
            })?;
        Ok(payload.into())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TmdbSearchResultPayload {
    results: Vec<TmdbSearchResultItemPayload>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbSearchResultItemPayload {
    id: i64,
    name: Option<String>,
    original_language: Option<String>,
    first_air_date: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbSeriesDetailsPayload {
    first_air_date: Option<String>,
    name: Option<String>,
    seasons: Vec<TmdbSeasonPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbSeasonPayload {
    episode_count: i64,
    season_number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbAlternativeTitlesPayload {
    results: Vec<TmdbAlternativeTitleItemPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TmdbAlternativeTitleItemPayload {
    title: String,
}

impl From<TmdbSearchResultPayload> for TmdbSearchResult {
    fn from(value: TmdbSearchResultPayload) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TmdbSearchResultItemPayload> for TmdbSearchResultItem {
    fn from(value: TmdbSearchResultItemPayload) -> Self {
        Self {
            id: value.id,
            name: value.name,
            original_language: value.original_language,
            first_air_date: value.first_air_date,
        }
    }
}

impl From<TmdbSeriesDetailsPayload> for TmdbSeriesDetails {
    fn from(value: TmdbSeriesDetailsPayload) -> Self {
        Self {
            first_air_date: value.first_air_date,
            name: value.name,
            seasons: value.seasons.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TmdbSeasonPayload> for TmdbSeason {
    fn from(value: TmdbSeasonPayload) -> Self {
        Self {
            episode_count: value.episode_count,
            season_number: value.season_number,
        }
    }
}

impl From<TmdbAlternativeTitlesPayload> for TmdbAlternativeTitles {
    fn from(value: TmdbAlternativeTitlesPayload) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TmdbAlternativeTitleItemPayload> for TmdbAlternativeTitleItem {
    fn from(value: TmdbAlternativeTitleItemPayload) -> Self {
        Self { title: value.title }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join(name),
        )
        .expect("read fixture")
    }

    #[test]
    fn maps_search_result_fixture() {
        let payload = serde_json::from_str::<TmdbSearchResultPayload>(&fixture(
            "tmdb_search_hyakkiyakosho.json",
        ))
        .expect("parse search");

        let result = TmdbSearchResult::from(payload);

        assert!(!result.results.is_empty());
        assert_eq!(result.results[0].id, 318478);
        assert_eq!(result.results[0].original_language.as_deref(), Some("ja"));
    }

    #[test]
    fn maps_series_details_fixture() {
        let payload = serde_json::from_str::<TmdbSeriesDetailsPayload>(&fixture(
            "tmdb_series_318478_hyakkiyakosho.json",
        ))
        .expect("parse details");

        let details = TmdbSeriesDetails::from(payload);

        assert_eq!(details.first_air_date.as_deref(), Some("2026-04-07"));
        assert!(!details.seasons.is_empty());
        assert!(details
            .seasons
            .iter()
            .any(|season| season.season_number == 1));
    }

    #[test]
    fn maps_alternative_titles_fixture() {
        let payload = serde_json::from_str::<TmdbAlternativeTitlesPayload>(&fixture(
            "tmdb_alternative_titles_318478_hyakkiyakosho.json",
        ))
        .expect("parse alternative titles");

        let titles = TmdbAlternativeTitles::from(payload);

        assert!(!titles.results.is_empty());
        assert!(titles.results.iter().any(|item| !item.title.is_empty()));
    }
}
