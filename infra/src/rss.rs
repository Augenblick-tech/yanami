use std::borrow::Cow;
use std::io::Cursor;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use domain::{feed::FeedSource, shared::error::DomainError};
use feed::contracts::{FeedData, FeedFetcher, FetchedFeedItem, ResolvedFeedSource};
use reqwest::{Client, Url};
use rss::{Channel, Item};
use sha1::{Digest, Sha1};

pub struct HttpFeedFetcher {
    client: Client,
}

impl HttpFeedFetcher {
    pub fn new() -> Result<Self, DomainError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| DomainError::external("rss client build failed", error))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch_url(&self, url: &str) -> Result<FeedData, DomainError> {
        self.fetch_url(url).await
    }
}

/// resolve_source 在只有 search_url 时，用这个值替换占位符以获得可请求的 RSS 页面。
const FALLBACK_SEARCH_KEYWORD: &str = "anime";

impl HttpFeedFetcher {
    pub async fn resolve_source(
        &self,
        source: &FeedSource,
    ) -> Result<ResolvedFeedSource, DomainError> {
        let feed_url: Cow<'_, str> = match source.site_url.as_deref() {
            Some(url) => url.into(),
            None => match source.search_url.as_deref() {
                Some(template) => template.replacen("{}", FALLBACK_SEARCH_KEYWORD, 1).into(),
                None => return Err(DomainError::InvariantViolation("feed has no url")),
            },
        };
        let channel = self.fetch_channel(&feed_url).await?;
        resolve_source_from_channel(&channel)
    }
}

impl HttpFeedFetcher {
    /// 获取 URL 并返回 FeedData。
    /// 429/5xx/网络错 → External (retryable)
    /// 400/401/403/404/500 → InvariantViolation (fatal)
    pub async fn fetch_url(&self, url: &str) -> Result<FeedData, DomainError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| DomainError::external("rss request failed", error))?;
        let status = response.status();
        if status == 400 || status == 401 || status == 403 || status == 404 || status == 500 {
            return Err(DomainError::InvariantViolation(
                "rss resource not accessible",
            ));
        }
        if !status.is_success() {
            response
                .bytes()
                .await
                .map_err(|error| DomainError::external("rss response body read failed", error))?;
            return Err(DomainError::external(
                "rss request failed",
                anyhow!("url={}, status={}", url, status),
            ));
        }
        let content = response
            .bytes()
            .await
            .map_err(|error| DomainError::external("rss response body read failed", error))?;
        let channel =
            Channel::read_from(std::io::Cursor::new(content.as_ref())).map_err(|error| {
                DomainError::external(
                    "rss channel parse failed",
                    anyhow!("url={}, error={}", url, error),
                )
            })?;
        let source = resolve_source_from_channel(&channel)?;
        let mut items = Vec::new();
        for item in channel.items() {
            if let Some(resource) = self.map_item(item).await? {
                items.push(resource);
            }
        }
        Ok(FeedData {
            source_key: source.source_key,
            items,
        })
    }

    async fn fetch_channel(&self, feed_url: &str) -> Result<Channel, DomainError> {
        let response = self
            .client
            .get(feed_url)
            .send()
            .await
            .map_err(|error| DomainError::external("rss request failed", error))?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let content = response
            .bytes()
            .await
            .map_err(|error| DomainError::external("rss response body read failed", error))?;

        if !status.is_success() {
            return Err(DomainError::external(
                "rss request returned non-success status",
                anyhow!(
                    "url={}, status={}, content_type={}, body_prefix={}",
                    feed_url,
                    status,
                    content_type.as_deref().unwrap_or("-"),
                    body_prefix(&content)
                ),
            ));
        }

        Channel::read_from(Cursor::new(content.as_ref())).map_err(|error| {
            DomainError::external(
                "rss channel parse failed",
                anyhow!(
                    "url={}, status={}, content_type={}, parse_error={}, body_prefix={}",
                    feed_url,
                    status,
                    content_type.as_deref().unwrap_or("-"),
                    error,
                    body_prefix(&content)
                ),
            )
        })
    }

    async fn map_item(&self, item: &Item) -> Result<Option<FetchedFeedItem>, DomainError> {
        let title = item
            .title()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(title) = title else {
            return Ok(None);
        };

        let source_url = item
            .enclosure()
            .map(|enclosure| enclosure.url().to_string())
            .or_else(|| item.link().map(ToOwned::to_owned));
        let Some(source_url) = source_url else {
            return Ok(None);
        };
        if !source_url.starts_with("magnet:?") && Url::parse(&source_url).is_err() {
            return Ok(None);
        }

        let torrent_content = self.load_torrent_content(&source_url).await?;

        Ok(Some(FetchedFeedItem {
            title: title.to_string(),
            source_url,
            torrent_content,
            published_at: parse_pub_date(item),
        }))
    }

    #[cfg(test)]
    async fn parse_for_test(content: &[u8]) -> Result<FeedData, DomainError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| DomainError::external("rss client build failed", error))?;
        let parser = Self { client };
        let channel = Channel::read_from(Cursor::new(content))
            .map_err(|error| DomainError::external("rss channel parse failed", error))?;
        let source = resolve_source_from_channel(&channel)?;
        let mut items = Vec::new();
        for item in channel.items() {
            if let Some(resource) = parser.map_item(item).await? {
                items.push(resource);
            }
        }
        Ok(FeedData {
            source_key: source.source_key,
            items,
        })
    }

    async fn load_torrent_content(&self, source_url: &str) -> Result<Option<Vec<u8>>, DomainError> {
        if source_url.starts_with("magnet:?") {
            return Ok(None);
        }

        let bytes = self
            .client
            .get(source_url)
            .send()
            .await
            .map_err(|error| DomainError::external("torrent request failed", error))?
            .bytes()
            .await
            .map_err(|error| DomainError::external("torrent response body read failed", error))?;
        Ok(Some(bytes.to_vec()))
    }
}

fn resolve_source_from_channel(channel: &Channel) -> Result<ResolvedFeedSource, DomainError> {
    let channel_title = channel.title().trim();
    if channel_title.is_empty() {
        return Err(DomainError::InvariantViolation(
            "rss channel title is missing",
        ));
    }
    let channel_link = channel.link().trim();
    if channel_link.is_empty() {
        return Err(DomainError::InvariantViolation(
            "rss channel link is missing",
        ));
    }
    Ok(ResolvedFeedSource {
        source_key: build_source_key(channel_title, channel_link),
    })
}

fn build_source_key(channel_title: &str, channel_link: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(channel_title.trim().to_lowercase().as_bytes());
    hasher.update(b"\n");
    hasher.update(channel_link.trim().to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn body_prefix(content: &[u8]) -> String {
    const MAX_CHARS: usize = 300;
    let text = String::from_utf8_lossy(content)
        .chars()
        .take(MAX_CHARS)
        .collect::<String>();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_pub_date(item: &Item) -> Option<i64> {
    let value = item.pub_date()?;
    chrono::DateTime::parse_from_rfc2822(value)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(value))
        .ok()
        .map(|dt| dt.timestamp())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use rss::ItemBuilder;

    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[tokio::test]
    async fn parses_real_snapshot_without_assigning_resource_identity() {
        let xml = fs::read(fixture_path("dmhy_search_hyakkiyakosho.xml")).expect("read fixture");

        let data = HttpFeedFetcher::parse_for_test(&xml)
            .await
            .expect("parse fixture");

        assert!(!data.items.is_empty());
        assert!(data.items[0].source_url.starts_with("magnet:?"));
        assert!(data.items[0].torrent_content.is_none());
    }

    #[test]
    fn parses_pub_date_and_ignores_invalid_values() {
        let item = ItemBuilder::default()
            .pub_date(Some("Tue, 07 Apr 2026 00:00:00 +0800".to_string()))
            .build();
        assert_eq!(parse_pub_date(&item), Some(1_775_491_200));

        let invalid = ItemBuilder::default()
            .pub_date(Some("invalid".to_string()))
            .build();
        assert!(parse_pub_date(&invalid).is_none());
    }
}
