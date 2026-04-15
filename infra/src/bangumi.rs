use std::time::Duration;

use anime::gateway::{
    BangumiAnimeSearchResultPage, BangumiCalendar, BangumiCalendarAnime, BangumiCalendarItem,
    BangumiSubject, BangumiSubjectSmall, BangumiWeekday,
};
use domain::shared::error::DomainError;
use reqwest::{
    header::{self, HeaderMap},
    Client,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug)]
pub struct BangumiClient {
    client: Client,
    base_url: String,
}

impl BangumiClient {
    pub fn new() -> Result<Self, anyhow::Error> {
        Self::with_base_url("https://api.bgm.tv")
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, anyhow::Error> {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, "yanami anime infrastructure".parse()?);
        Ok(Self {
            client: Client::builder()
                .default_headers(headers)
                .timeout(Duration::from_secs(30))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn get_calendar(&self) -> Result<Vec<BangumiCalendar>, DomainError> {
        let payload = self
            .client
            .get(format!("{}/calendar", self.base_url))
            .send()
            .await
            .map_err(|error| DomainError::external("bangumi calendar request failed", error))?
            .json::<Vec<BangumiCalendarPayload>>()
            .await
            .map_err(|error| DomainError::external("bangumi calendar decode failed", error))?;
        Ok(payload.into_iter().map(Into::into).collect())
    }

    pub async fn get_subject(&self, id: i64) -> Result<Option<BangumiSubject>, DomainError> {
        let response = self
            .client
            .get(format!("{}/v0/subjects/{id}", self.base_url))
            .send()
            .await
            .map_err(|error| DomainError::external("bangumi subject request failed", error))?;
        if response.status() != 200 {
            return Ok(None);
        }
        let payload = response
            .json::<BangumiSubjectPayload>()
            .await
            .map_err(|error| DomainError::external("bangumi subject decode failed", error))?;
        Ok(Some(payload.into()))
    }

    pub async fn get_calendar_anime(&self) -> Result<Vec<BangumiCalendarAnime>, DomainError> {
        let calendar = self.get_calendar().await?;
        let mut items = Vec::new();
        for day in calendar {
            for item in day.items {
                let Some(subject) = self.get_subject(item.id).await? else {
                    warn!(anime_id = item.id, anime_name = %item.name, "bangumi subject missing");
                    continue;
                };
                let air_date = match subject.air_date.or(item.air_date.clone()) {
                    Some(value) => value,
                    None => {
                        warn!(anime_id = item.id, anime_name = %item.name, "bangumi air date missing");
                        continue;
                    }
                };
                items.push(BangumiCalendarAnime {
                    id: item.id,
                    name: item.name,
                    weekday: day.weekday.id,
                    eps: subject.eps,
                    air_date,
                });
            }
        }
        Ok(items)
    }

    pub async fn search_anime(
        &self,
        keyword: &str,
        limit: u32,
        offset: u32,
    ) -> Result<BangumiAnimeSearchResultPage, DomainError> {
        let payload = self
            .client
            .post(format!(
                "{}/v0/search/subjects?limit={limit}&offset={offset}",
                self.base_url
            ))
            .json(&BangumiSubjectSearchRequestPayload {
                keyword,
                sort: "match",
                filter: BangumiSubjectSearchFilterPayload {
                    subject_type: vec![2],
                },
            })
            .send()
            .await
            .map_err(|error| DomainError::external("bangumi anime search request failed", error))?
            .json::<BangumiAnimeSearchResultPagePayload>()
            .await
            .map_err(|error| DomainError::external("bangumi anime search decode failed", error))?;
        Ok(payload.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BangumiCalendarPayload {
    weekday: BangumiWeekdayPayload,
    items: Vec<BangumiCalendarItemPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BangumiWeekdayPayload {
    id: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BangumiCalendarItemPayload {
    id: i64,
    name: String,
    air_date: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BangumiSubjectPayload {
    name: Option<String>,
    name_cn: Option<String>,
    eps: Option<i64>,
    #[serde(alias = "date")]
    air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BangumiSubjectSearchRequestPayload<'a> {
    keyword: &'a str,
    sort: &'a str,
    filter: BangumiSubjectSearchFilterPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BangumiSubjectSearchFilterPayload {
    #[serde(rename = "type")]
    subject_type: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct BangumiAnimeSearchResultPagePayload {
    total: u32,
    limit: Option<u32>,
    offset: Option<u32>,
    data: Vec<BangumiSubjectSmallPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct BangumiSubjectSmallPayload {
    id: i64,
    #[serde(rename = "type")]
    subject_type: i64,
    name: String,
    #[serde(default)]
    name_cn: String,
    #[serde(default)]
    summary: String,
    air_date: Option<String>,
    air_weekday: Option<i64>,
    eps: Option<i64>,
}

impl From<BangumiCalendarPayload> for BangumiCalendar {
    fn from(value: BangumiCalendarPayload) -> Self {
        Self {
            weekday: BangumiWeekday {
                id: value.weekday.id,
            },
            items: value.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BangumiCalendarItemPayload> for BangumiCalendarItem {
    fn from(value: BangumiCalendarItemPayload) -> Self {
        Self {
            id: value.id,
            name: value.name,
            air_date: value.air_date,
        }
    }
}

impl From<BangumiSubjectPayload> for BangumiSubject {
    fn from(value: BangumiSubjectPayload) -> Self {
        Self {
            name: value.name,
            name_cn: value.name_cn,
            eps: value.eps,
            air_date: value.air_date,
        }
    }
}

impl From<BangumiAnimeSearchResultPagePayload> for BangumiAnimeSearchResultPage {
    fn from(value: BangumiAnimeSearchResultPagePayload) -> Self {
        Self {
            total: value.total,
            limit: value.limit.unwrap_or(value.data.len() as u32),
            offset: value.offset.unwrap_or(0),
            data: value.data.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BangumiSubjectSmallPayload> for BangumiSubjectSmall {
    fn from(value: BangumiSubjectSmallPayload) -> Self {
        Self {
            id: value.id,
            subject_type: value.subject_type,
            name: value.name,
            name_cn: value.name_cn,
            summary: value.summary,
            air_date: value.air_date,
            air_weekday: value.air_weekday,
            eps: value.eps,
        }
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
    fn maps_calendar_payload_fixture() {
        let payloads =
            serde_json::from_str::<Vec<BangumiCalendarPayload>>(&fixture("bangumi_calendar.json"))
                .expect("parse calendar");

        let calendar = payloads
            .into_iter()
            .map(BangumiCalendar::from)
            .collect::<Vec<_>>();

        assert_eq!(calendar.len(), 7);
        assert!(calendar.iter().any(|day| !day.items.is_empty()));
        assert_eq!(calendar[0].weekday.id, 1);
    }

    #[test]
    fn maps_subject_payload_fixture() {
        let payload = serde_json::from_str::<BangumiSubjectPayload>(&fixture(
            "bangumi_subject_627137_hyakkiyakosho.json",
        ))
        .expect("parse subject");

        let subject = BangumiSubject::from(payload);

        assert_eq!(subject.eps, Some(12));
        assert_eq!(subject.air_date.as_deref(), Some("2026-04-07"));
    }

    #[test]
    fn maps_search_payload_fixture() {
        let payload = serde_json::from_str::<BangumiAnimeSearchResultPagePayload>(&fixture(
            "bangumi_search_hyakkiyakosho.json",
        ))
        .expect("parse search");

        let result = BangumiAnimeSearchResultPage::from(payload);

        assert!(result.total >= 1);
        assert!(!result.data.is_empty());
        assert!(result.data.iter().any(|item| item.subject_type == 2));
        assert!(result.data.iter().any(|item| item.name.contains("百鬼")));
    }
}
