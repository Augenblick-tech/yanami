use anyhow::Result;
use async_trait::async_trait;
use base32::Alphabet;
use chrono::Utc;
use reqwest::Client;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{collections::HashMap, io::Cursor, sync::Arc};
use tracing::{error, warn};
use url::Url;

use crate::entity::{
    cap::FeedFetcher,
    model::{FeedData, FeedFetchError, FeedItem},
};

use regex::Regex;
use std::sync::OnceLock;

/// 判断是否为合集/打包类资源（准确排除单集 END/完结 标识）
fn is_collection_resource(title: &str) -> bool {
    static COLLECTION_RE: OnceLock<Regex> = OnceLock::new();

    let re = COLLECTION_RE.get_or_init(|| {
        Regex::new(r#"(?x)
            # 1. 明确的合集关键词（不管是否有数字，直接命中）
            # 排除 standalone 的 Season 1 等，只有复数 Seasons 1-2 或搭配 Complete/Batch 才算
            (?i:\b(?:Batch|Complete|Collection|Seasons?\s*\d+\s*[-~至]\s*\d+)\b|合集|全集|全套|全话|整季|季度全集|打包) |
            
            # 2. 括号内的集数区间：[01-12], [01~24], (01-13), [01v2-12v2], [Ep01-Ep12]
            # 关键保护：严格限制数字必须有 '-' 或 '~' 连接，如 [12 END] 没有连字符就不会命中
            # 并且限制首个数字为 1~3 位，防止匹配到日期 [2025-01-08]
            (?:\[|\() \s* (?:[Eｅ][Pｐ]?)?\d{1,3}(?:[vV]\d)? \s* [-~至] \s* (?:[Eｅ][Pｐ]?)?\d{1,3}(?:[vV]\d)? \s* (?:END|完结|完)? \s* (?:\]|\)) |
            
            # 3. 中文集数区间格式：第01-12话, 第01~24集(完结)
            第 \s* \d{1,3} \s* [-~至] \s* \d{1,3} \s* [话集] |
            
            # 4. 无括号但带有连字符且以 END/完结 结尾的区间： " 01-12 END ", "- 01~24 完结 -"
            # 必须满足 [数字]-[数字]+[END/完结]，从而安全放过 " - 12 END " (无连字符区间)
            (?:\s|_|-)(?:[Eｅ][Pｐ]?)?\d{2,3}(?:[vV]\d)?\s*[-~至]\s*(?:[Eｅ][Pｐ]?)?\d{2,3}(?:[vV]\d)?\s*(?:END|完结|全集)(?:\s|_|\[|\(|$)
        "#).expect("Invalid regex for collection filtering")
    });

    re.is_match(title)
}

#[async_trait]
pub trait FeedItemRepository: Send + Sync {
    // 根据url获取info_hash，如果url不存在对应的hash则返回值中不包含
    async fn get_url_info_hash(&self, urls: Vec<&str>) -> Result<HashMap<String, [u8; 20]>>;
}

struct ParsedFeed {
    pub source_key: String,
    pub items: Vec<ParsedItem>,
}

struct ParsedItem {
    pub title: String,
    pub source_url: String,
    pub resource_url: String,
    pub published_at: i64,
    pub info_hash: [u8; 20],
}

#[derive(Clone)]
pub struct HttpFeedFetcher {
    client: Client,
    repo: Arc<dyn FeedItemRepository>,
}

impl HttpFeedFetcher {
    pub fn new(client: Client, repo: Arc<dyn FeedItemRepository>) -> Self {
        Self { client, repo }
    }

    fn handle_status_error(url: &str, status: reqwest::StatusCode) -> FeedFetchError {
        if matches!(status.as_u16(), 400 | 401 | 403 | 404 | 500) {
            FeedFetchError::Inaccessible(format!("url={}, status={}", url, status))
        } else {
            FeedFetchError::Retryable(format!("url={}, status={}", url, status))
        }
    }
}

#[async_trait]
impl FeedFetcher for HttpFeedFetcher {
    async fn fetch_url(&self, url: &str) -> Result<FeedData, FeedFetchError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| FeedFetchError::Retryable(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(Self::handle_status_error(url, status));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| FeedFetchError::Retryable(format!("failed to read body: {}", e)))?;

        let parsed = parse_feed(&body, url)?;

        // 搜集所有 URL
        let all_urls: Vec<&str> = parsed
            .items
            .iter()
            .map(|item| item.resource_url.as_str())
            .collect();

        // 批量从 Repo 读取已缓存的 InfoHash，如果 Repo 报错则降级为空 Map（不阻断流程）
        let cached_hashes = if !all_urls.is_empty() {
            match self.repo.get_url_info_hash(all_urls).await {
                Ok(map) => map,
                Err(e) => {
                    error!(feed_url = %url, error = %e, "failed to query info_hash cache from repo, falling back to network");
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        let mut items = Vec::with_capacity(parsed.items.len());
        for mut item in parsed.items {
            // 命中缓存的旧数据不返回给上游
            if cached_hashes.contains_key(&item.resource_url) {
                continue;
            }
            if item.info_hash == [0u8; 20] {
                if let Some(hash) = self.download_torrent_hash(url, &item).await {
                    item.info_hash = hash;
                } else {
                    continue; // 彻底获取失败，跳过该脏数据
                }
            }

            items.push(FeedItem {
                title: item.title,
                source_url: item.source_url,
                resource_url: item.resource_url,
                published_at: item.published_at,
                info_hash: item.info_hash,
            });
        }

        Ok(FeedData {
            source_key: parsed.source_key,
            items,
        })
    }

    async fn get_source_key(&self, url: &str) -> Result<String, FeedFetchError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| FeedFetchError::Retryable(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(Self::handle_status_error(url, status));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| FeedFetchError::Retryable(format!("failed to read body: {}", e)))?;

        let channel = Channel::read_from(Cursor::new(body.as_ref()))
            .map_err(|e| FeedFetchError::InvalidData(e.to_string()))?;

        let title = channel.title().trim();
        if title.is_empty() {
            return Err(FeedFetchError::InvalidData(format!(
                "missing title: {}",
                url
            )));
        }
        let link = channel.link().trim();
        if link.is_empty() {
            return Err(FeedFetchError::InvalidData(format!(
                "missing link: {}",
                url
            )));
        }

        Ok(build_source_key(title, link))
    }
}

impl HttpFeedFetcher {
    async fn download_torrent_hash(&self, feed_url: &str, item: &ParsedItem) -> Option<[u8; 20]> {
        let bytes = match self.client.get(&item.resource_url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    error!(feed_url = %feed_url, title = %item.title, resource_url = %item.resource_url, error = %e, "failed to read torrent bytes");
                    return None;
                }
            },
            Err(e) => {
                error!(feed_url = %feed_url, title = %item.title, resource_url = %item.resource_url, error = %e, "failed to download torrent");
                return None;
            }
        };

        match torrent_info_hash(&bytes) {
            Ok(h) => Some(h),
            Err(e) => {
                error!(feed_url = %feed_url, title = %item.title, resource_url = %item.resource_url, error = %e, "failed to compute torrent hash");
                None
            }
        }
    }
}

fn parse_feed(content: &[u8], feed_url: &str) -> Result<ParsedFeed, FeedFetchError> {
    let channel = Channel::read_from(Cursor::new(content))
        .map_err(|e| FeedFetchError::InvalidData(e.to_string()))?;

    let channel_title = channel.title().trim();
    if channel_title.is_empty() {
        return Err(FeedFetchError::InvalidData(format!(
            "missing title: {}",
            feed_url
        )));
    }
    let channel_link = channel.link().trim();
    if channel_link.is_empty() {
        return Err(FeedFetchError::InvalidData(format!(
            "missing link: {}",
            feed_url
        )));
    }

    let source_key = build_source_key(channel_title, channel_link);
    let mut items = Vec::new();

    for item in channel.items() {
        let Some(source_url) = item.link().map(str::trim).filter(|s| !s.is_empty()) else {
            return Err(FeedFetchError::InvalidData(format!(
                "item missing link: {}",
                feed_url
            )));
        };

        let Some(title) = item.title().map(str::trim).filter(|s| !s.is_empty()) else {
            warn!(feed_url = %feed_url, link = %source_url, "skipping rss item without title");
            continue;
        };

        // 过滤掉集合类资源
        if is_collection_resource(title) {
            warn!(feed_url = %feed_url, title = %title, "skipping collection/batch resource");
            continue;
        }

        let Some(resource_url) = item.enclosure().map(|e| e.url()) else {
            warn!(feed_url = %feed_url, title = %title, "skipping rss item without enclosure url");
            continue;
        };

        let info_hash = if resource_url.starts_with("magnet:?") {
            match magnet_info_hash(resource_url) {
                Ok(Some(hash)) => hash,
                Ok(None) => {
                    warn!(feed_url = %feed_url, title = %title, resource_url = %resource_url, "skipping magnet missing valid btih");
                    continue;
                }
                Err(e) => {
                    error!(feed_url = %feed_url, title = %title, resource_url = %resource_url, error = %e, "failed to extract info_hash from magnet");
                    continue;
                }
            }
        } else if Url::parse(resource_url).is_ok() {
            [0u8; 20]
        } else {
            warn!(feed_url = %feed_url, title = %title, resource_url = %resource_url, "skipping unparseable resource url");
            continue;
        };

        let published_at = extract_pub_date(item).unwrap_or_else(|| {
            warn!(feed_url = %feed_url, title = %title, pub_date_raw = ?item.pub_date(), "fallback to current time");
            Utc::now().timestamp()
        });

        items.push(ParsedItem {
            title: title.to_string(),
            source_url: source_url.to_string(),
            resource_url: resource_url.to_string(),
            published_at,
            info_hash,
        });
    }

    Ok(ParsedFeed { source_key, items })
}

fn build_source_key(title: &str, link: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(title.trim().to_lowercase().as_bytes());
    hasher.update(b"\n");
    hasher.update(link.trim().to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn magnet_info_hash(resource_url: &str) -> Result<Option<[u8; 20]>, anyhow::Error> {
    let Ok(url) = Url::parse(resource_url) else {
        return Ok(None);
    };
    if url.scheme() != "magnet" {
        return Ok(None);
    }
    let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "xt") else {
        return Ok(None);
    };
    let Some(hash) = value.strip_prefix("urn:btih:") else {
        return Ok(None);
    };

    let bytes = if hash.len() <= 32 {
        base32::decode(Alphabet::Rfc4648 { padding: true }, &hash.to_uppercase())
            .ok_or_else(|| anyhow::anyhow!("invalid base32 btih"))?
    } else {
        hex::decode(hash)?
    };

    let arr: [u8; 20] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("btih must be 20 bytes"))?;
    Ok(Some(arr))
}

fn torrent_info_hash(bytes: &[u8]) -> Result<[u8; 20], anyhow::Error> {
    let torrent: TorrentFile = serde_bencode::from_bytes(bytes)?;
    let info = serde_bencode::to_bytes(&torrent.info)?;
    let mut hasher = Sha1::new();
    hasher.update(info);
    let hash: [u8; 20] = hasher
        .finalize()
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("sha1 must produce 20 bytes"))?;
    Ok(hash)
}

fn extract_pub_date(item: &Item) -> Option<i64> {
    if let Some(val) = item.pub_date()
        && let Ok(dt) = chrono::DateTime::parse_from_rfc2822(val)
            .or_else(|_| chrono::DateTime::parse_from_rfc3339(val))
        {
            return Some(dt.timestamp());
        }

    let ext_val = item
        .extensions()
        .get("mikan")
        .and_then(|m| m.get("torrent"))
        .and_then(|v| v.first())
        .and_then(|e| e.children().get("pubDate"))
        .and_then(|v| v.first())
        .and_then(|e| e.value());

    if let Some(val) = ext_val
        && let Ok(naive) = chrono::NaiveDateTime::parse_from_str(val, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(val, "%Y-%m-%dT%H:%M:%S"))
        {
            return Some(naive.and_utc().timestamp());
        }

    None
}

#[derive(Debug, Deserialize)]
struct TorrentFile {
    info: TorrentInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct TorrentInfo(serde_bencode::value::Value);
