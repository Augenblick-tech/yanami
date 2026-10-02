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

        Ok(build_source_key(link))
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
    let parser = get_feed_parser(feed_url);
    let xml_str = parser.preprocess_xml(content);

    let channel = Channel::read_from(Cursor::new(xml_str.as_bytes()))
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

    let source_key = build_source_key(channel_link);
    let mut items = Vec::new();

    for item in channel.items() {
        let Some(source_url) = parser.extract_source_url(item) else {
            warn!(feed_url = %feed_url, title = ?item.title(), "skipping rss item missing source link");
            continue;
        };

        let Some(title) = item.title().map(str::trim).filter(|s| !s.is_empty()) else {
            warn!(feed_url = %feed_url, link = %source_url, "skipping rss item without title");
            continue;
        };

        // 过滤掉集合类资源
        if is_collection_resource(title) {
            tracing::debug!(feed_url = %feed_url, title = %title, "skipping collection/batch resource");
            continue;
        }

        let Some(resource_url) = parser.extract_resource_url(item) else {
            warn!(feed_url = %feed_url, title = %title, "skipping rss item without resource url");
            continue;
        };

        let info_hash = match parser.extract_info_hash(item, &resource_url) {
            Ok(hash) => hash,
            Err(e) => {
                warn!(feed_url = %feed_url, title = %title, resource_url = %resource_url, error = %e, "failed to get info_hash");
                continue;
            }
        };

        let published_at = parser.extract_pub_date(item).unwrap_or_else(|| {
            warn!(feed_url = %feed_url, title = %title, pub_date_raw = ?item.pub_date(), "fallback to current time");
            Utc::now().timestamp()
        });

        items.push(ParsedItem {
            title: title.to_string(),
            source_url,
            resource_url,
            published_at,
            info_hash,
        });
    }

    Ok(ParsedFeed { source_key, items })
}

fn build_source_key(link: &str) -> String {
    let mut hasher = Sha1::new();
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

trait FeedParser {
    fn preprocess_xml(&self, content: &[u8]) -> String {
        String::from_utf8_lossy(content).into_owned()
    }

    fn extract_source_url(&self, item: &Item) -> Option<String> {
        item.link()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_resource_url(&self, item: &Item) -> Option<String> {
        item.enclosure().map(|e| e.url().to_string())
    }

    fn extract_info_hash(&self, _item: &Item, resource_url: &str) -> Result<[u8; 20], String> {
        if resource_url.starts_with("magnet:?") {
            match magnet_info_hash(resource_url) {
                Ok(Some(hash)) => Ok(hash),
                Ok(None) => Err("skipping magnet missing valid btih".to_string()),
                Err(e) => Err(format!("failed to extract info_hash from magnet: {}", e)),
            }
        } else if Url::parse(resource_url).is_ok() {
            Ok([0u8; 20])
        } else {
            Err("skipping unparseable resource url".to_string())
        }
    }

    fn extract_pub_date(&self, item: &Item) -> Option<i64> {
        if let Some(val) = item.pub_date()
            && let Ok(dt) = chrono::DateTime::parse_from_rfc2822(val)
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(val))
        {
            return Some(dt.timestamp());
        }
        None
    }
}

struct DefaultParser;
impl FeedParser for DefaultParser {}

struct MikanParser;
impl FeedParser for MikanParser {
    fn preprocess_xml(&self, content: &[u8]) -> String {
        let xml_str = String::from_utf8_lossy(content).into_owned();
        // 修复 Mikan 的 RSS 数据：Mikan 移除了 'mikan' 前缀，转而对 `<torrent>` 使用默认命名空间。
        // 由于 `rss` 库在解析时会直接丢弃没有前缀的非标准标签，因此我们必须在解析前将前缀重新注入。
        let mut patched = xml_str.replace(
            "<rss version=\"2.0\">",
            "<rss version=\"2.0\" xmlns:mikan=\"https://mikanani.me/0.1/\">",
        );
        patched = patched.replace(
            "<torrent xmlns=\"https://mikanani.me/0.1/\">",
            "<mikan:torrent>",
        );
        patched = patched.replace("</torrent>", "</mikan:torrent>");
        patched
    }

    fn extract_pub_date(&self, item: &Item) -> Option<i64> {
        if let Some(timestamp) = DefaultParser.extract_pub_date(item) {
            return Some(timestamp);
        }

        let ext_val = item
            .extensions()
            .get("mikan")
            .or_else(|| item.extensions().get(""))
            .or_else(|| item.extensions().get("torrent"))
            .and_then(|m| m.get("torrent"))
            .and_then(|v| v.first())
            .and_then(|e| e.children().get("pubDate"))
            .and_then(|v| v.first())
            .and_then(|e| e.value());

        if let Some(val) = ext_val {
            let parsed = chrono::NaiveDateTime::parse_from_str(val, "%Y-%m-%dT%H:%M:%S.%f")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(val, "%Y-%m-%dT%H:%M:%S"));
            if let Ok(naive) = parsed {
                return Some(naive.and_utc().timestamp());
            }
        }
        None
    }
}

struct NyaaParser;
impl FeedParser for NyaaParser {
    fn extract_source_url(&self, item: &Item) -> Option<String> {
        item.guid().map(|g| g.value.trim().to_string())
    }

    fn extract_resource_url(&self, item: &Item) -> Option<String> {
        item.link()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_info_hash(&self, item: &Item, resource_url: &str) -> Result<[u8; 20], String> {
        if let Some(hash_str) = item
            .extensions()
            .get("nyaa")
            .and_then(|m| m.get("infoHash"))
            .and_then(|v| v.first())
            .and_then(|e| e.value())
            && let Ok(bytes) = hex::decode(hash_str)
            && bytes.len() == 20
        {
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&bytes);
            return Ok(arr);
        }
        DefaultParser.extract_info_hash(item, resource_url)
    }
}

fn get_feed_parser(feed_url: &str) -> Box<dyn FeedParser> {
    if feed_url.contains("mikanani.me") {
        Box::new(MikanParser)
    } else if feed_url.contains("nyaa.si") {
        Box::new(NyaaParser)
    } else {
        Box::new(DefaultParser)
    }
}

#[derive(Debug, Deserialize)]
struct TorrentFile {
    info: TorrentInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct TorrentInfo(serde_bencode::value::Value);

#[cfg(test)]
mod tests {
    use super::*;

    // 真实 Mikan 频道的 RSS：https://mikanani.me/RSS/Classic
    // 仅裁剪为「频道头 + 前两条 item」，字段内容保持原样未改动。
    const MIKAN_XML: &str = r####"<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>Mikan Project - 列表模式</title><link>http://mikanani.me/RSS/Classic</link><description>Mikan Project - 列表模式</description><item><guid isPermaLink="false">[LoliHouse] 黑化吧！圣女大人 第二季 / 伤だらけ圣女より报复をこめて Season 2 / Kizu darake Seijo yori Houfuku wo Komete Season 2 - 01 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕]</guid><link>https://mikanani.me/Home/Episode/d7c45419815d1af4e5542fc7edca3f7ec3cadfdf</link><title>[LoliHouse] 黑化吧！圣女大人 第二季 / 伤だらけ圣女より报复をこめて Season 2 / Kizu darake Seijo yori Houfuku wo Komete Season 2 - 01 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕]</title><description>[LoliHouse] 黑化吧！圣女大人 第二季 / 伤だらけ圣女より报复をこめて Season 2 / Kizu darake Seijo yori Houfuku wo Komete Season 2 - 01 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕][563.9MB]</description><torrent xmlns="https://mikanani.me/0.1/"><link>https://mikanani.me/Home/Episode/d7c45419815d1af4e5542fc7edca3f7ec3cadfdf</link><contentLength>591292032</contentLength><pubDate>2026-10-02T16:52:00</pubDate></torrent><enclosure type="application/x-bittorrent" length="591292032" url="https://mikanani.me/Download/20261002/d7c45419815d1af4e5542fc7edca3f7ec3cadfdf.torrent" /></item><item><guid isPermaLink="false">[绿茶字幕组] FX战士久留美 / FX Senshi Kurumi-chan [01v2][WebRip][1080p][繁日内嵌]</guid><link>https://mikanani.me/Home/Episode/73d2014cf6522cfb9f372eec4fa8ed889076dc3c</link><title>[绿茶字幕组] FX战士久留美 / FX Senshi Kurumi-chan [01v2][WebRip][1080p][繁日内嵌]</title><description>[绿茶字幕组] FX战士久留美 / FX Senshi Kurumi-chan [01v2][WebRip][1080p][繁日内嵌][433.0 MB]</description><torrent xmlns="https://mikanani.me/0.1/"><link>https://mikanani.me/Home/Episode/73d2014cf6522cfb9f372eec4fa8ed889076dc3c</link><contentLength>454033408</contentLength><pubDate>2026-10-02T16:33:41.704143</pubDate></torrent><enclosure type="application/x-bittorrent" length="454033408" url="https://mikanani.me/Download/20261002/73d2014cf6522cfb9f372eec4fa8ed889076dc3c.torrent" /></item></channel></rss>"####;

    // 真实 Nyaa 的 RSS：https://nyaa.si/?page=rss
    // 仅裁剪为「频道头 + 前两条 item」，字段内容保持原样未改动。
    const NYAA_XML: &str = r####"<rss xmlns:atom="http://www.w3.org/2005/Atom" xmlns:nyaa="https://nyaa.si/xmlns/nyaa" version="2.0">
	<channel>
		<title>Nyaa - Home - Torrent File RSS</title>
		<description>RSS Feed for Home</description>
		<link>https://nyaa.si/</link>
		<atom:link href="https://nyaa.si/?page=rss" rel="self" type="application/rss+xml" />
		<item>
			<title>[Doomdos] - 트릭컬 만화동산 - 01 [1080P LFTLNET WEB-DL]</title>
				<link>https://nyaa.si/download/2168588.torrent</link>
				<guid isPermaLink="true">https://nyaa.si/view/2168588</guid>
				<pubDate>Fri, 02 Oct 2026 08:53:46 -0000</pubDate>

				<nyaa:seeders>9</nyaa:seeders>
				<nyaa:leechers>3</nyaa:leechers>
				<nyaa:downloads>6</nyaa:downloads>
				<nyaa:infoHash>d18e3d4491ddaee333cc23786ea264c1eac225cc</nyaa:infoHash>
			<nyaa:categoryId>1_3</nyaa:categoryId>
			<nyaa:category>Anime - Non-English-translated</nyaa:category>
			<nyaa:size>122.1 MiB</nyaa:size>
			<nyaa:comments>0</nyaa:comments>
			<nyaa:trusted>No</nyaa:trusted>
			<nyaa:remake>No</nyaa:remake>
			<description><![CDATA[<a href="https://nyaa.si/view/2168588">#2168588 | [Doomdos] - 트릭컬 만화동산 - 01 [1080P LFTLNET WEB-DL]</a> | 122.1 MiB | Anime - Non-English-translated | D18E3D4491DDAEE333CC23786EA264C1EAC225CC]]></description>
		</item><item>
			<title>[LoliHouse] 黑化吧！圣女大人 第二季 / 傷だらけ聖女より報復をこめて Season 2 / Kizu darake Seijo yori Houfuku wo Komete Season 2 - 01 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕]</title>
				<link>https://nyaa.si/download/2168587.torrent</link>
				<guid isPermaLink="true">https://nyaa.si/view/2168587</guid>
				<pubDate>Fri, 02 Oct 2026 08:52:08 -0000</pubDate>

				<nyaa:seeders>21</nyaa:seeders>
				<nyaa:leechers>12</nyaa:leechers>
				<nyaa:downloads>26</nyaa:downloads>
				<nyaa:infoHash>d7c45419815d1af4e5542fc7edca3f7ec3cadfdf</nyaa:infoHash>
			<nyaa:categoryId>1_3</nyaa:categoryId>
			<nyaa:category>Anime - Non-English-translated</nyaa:category>
			<nyaa:size>563.9 MiB</nyaa:size>
			<nyaa:comments>0</nyaa:comments>
			<nyaa:trusted>No</nyaa:trusted>
			<nyaa:remake>No</nyaa:remake>
			<description><![CDATA[<a href="https://nyaa.si/view/2168587">#2168587 | [LoliHouse] 黑化吧！圣女大人 第二季 / 傷だらけ聖女より報復をこめて Season 2 / Kizu darake Seijo yori Houfuku wo Komete Season 2 - 01 [WebRip 1080p HEVC-10bit AAC][简繁内封字幕]</a> | 563.9 MiB | Anime - Non-English-translated | D7C45419815D1AF4E5542FC7EDCA3F7EC3CADFDF]]></description>
		</item></channel></rss>"####;

    // 真实普通 RSS（播客）：https://changelog.com/podcast/feed
    // 仅裁剪为「频道头 + 第一条 item」，字段内容保持原样未改动。
    const PLAIN_RSS: &str = r####"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd" xmlns:podcast="https://podcastindex.org/namespace/1.0" xmlns:psc="http://podlove.org/simple-chapters">
  <channel>
    <title>The Changelog: Software Development, Open Source</title>
    <copyright>All rights reserved</copyright>
    <link>https://changelog.com/podcast</link>
    <atom:link type="application/rss+xml" rel="self" href="https://changelog.com/podcast/feed"/>
    <atom:link type="text/html" rel="alternate" href="https://changelog.com/podcast"/>
    <language>en-us</language>
    <description>Software&apos;s best weekly news brief, deep technical interviews &amp; talk show.</description>
    <itunes:summary>Software&apos;s best weekly news brief, deep technical interviews &amp; talk show.</itunes:summary>
    <itunes:explicit>false</itunes:explicit>
    <itunes:image href="https://cdn.changelog.com/static/images/podcasts/podcast-original-f16d0363067166f241d080ee2e2d4a28.png"/>
    <itunes:author>Changelog Media</itunes:author>
    <itunes:owner>
      <itunes:name>Changelog Media</itunes:name>
      <itunes:email>editors@changelog.com</itunes:email>
    </itunes:owner>
    <itunes:keywords>changelog, open source, software, development, code, programming, hacker, change log, software engineering</itunes:keywords>
    <itunes:category text="Technology"/>
    <podcast:funding url="https://changelog.com/++">Support our work by joining Changelog++</podcast:funding>
    <podcast:person role="host" img="https://cdn.changelog.com/uploads/avatars/people/Qo/avatar_large.jpg?v=63760280419" href="https://changelog.com/person/adamstac">Adam Stacoviak</podcast:person>
    <item>
      <title>Forking Cal.com to closed source (Interview)</title>
      <guid isPermaLink="false">changelog.com/1/2836</guid>
      <link>https://changelog.com/podcast/685</link>
      <pubDate>Thu, 03 Sep 2026 20:00:00 +0000</pubDate>
      <enclosure type="audio/mpeg" length="165091080" url="https://op3.dev/e/https://pscrb.fm/rss/p/https://cdn.changelog.com/uploads/podcast/685/the-changelog-685.mp3"/>
      <description>This week I&apos;m joined by Peer Richelsen, co-founder of Cal.com. What if the majority of open source repositories are already compromised and we just don&apos;t know it yet? That&apos;s the theory Peer brings to the table this week. We dig into how AI has flattened the knowledge graph to the point that a 16-year-old can vibe hack a power station just as easily as their mom can vibe code an iOS app, why the reporting culture that has kept open source safe all these years is collapsing under AI generated noise, Cal.com&apos;s move to fork its own codebase and take the sensitive parts private, and the eye opening reality that shipping &quot;$1 of AI tokens for pennies on the dollar&quot; is now a common startup business model.</description>
      <itunes:episodeType>full</itunes:episodeType>
      <itunes:image href="https://cdn.changelog.com/uploads/covers/changelog-interviews-original.png?v=63848368174"/>
      <itunes:duration>1:54:32</itunes:duration>
      <itunes:explicit>false</itunes:explicit>
      <podcast:person role="host" img="https://cdn.changelog.com/uploads/avatars/people/Qo/avatar_large.jpg?v=63760280419" href="https://changelog.com/person/adamstac">Adam Stacoviak</podcast:person>
      <podcast:person role="guest" img="https://cdn.changelog.com/uploads/avatars/people/0PXPe/avatar_large.jpg?v=63955690029" href="https://changelog.com/person/peer">Peer Richelsen</podcast:person>
      <podcast:chapters type="application/json+chapters" url="https://changelog.com/podcast/685/chapters"/>
      <psc:chapters version="1.1" xmlns="http://podlove.org/simple-chapters">
        <psc:chapter start="0" title="It's The Changelog!"/>
        <psc:chapter start="83" title="Sponsor: Coder.com" href="https://coder.com/"/>
        <psc:chapter start="270" title="It's about time"/>
        <psc:chapter start="421" title="AI reshuffles the startup playbook"/>
        <psc:chapter start="795" title="Slop is overwhelming maintainers"/>
        <psc:chapter start="1062" title="The security pendulum swings back"/>
        <psc:chapter start="1538" title="Most open source repos are compromised"/>
        <psc:chapter start="1841" title="Commercial open source as a target"/>
        <psc:chapter start="2176" title="Sponsor: Buildkite" href="https://buildkite.com/"/>
        <psc:chapter start="2343" title="Forking Cal.com's code private"/>
        <psc:chapter start="2770" title="Other COSS companies struggling"/>
        <psc:chapter start="3131" title="Prescription: go private, patch vulnerabilities"/>
        <psc:chapter start="3272" title="Is GitHub at risk?"/>
        <psc:chapter start="3615" title="Sponsor: WorkOS" href="https://workos.com"/>
        <psc:chapter start="3750" title="Mitchell Hashimoto's GitHub fix"/>
        <psc:chapter start="3902" title="The AI coding landscape"/>
        <psc:chapter start="4267" title="The autonomous agent future"/>
        <psc:chapter start="4561" title="Cal.com's growth story"/>
        <psc:chapter start="4674" title="Burning money to grow"/>
        <psc:chapter start="5149" title="Growing SaaS without AI"/>
        <psc:chapter start="5281" title="The reschedule UX problem"/>
        <psc:chapter start="5546" title="Racing toward 10 million ARR"/>
        <psc:chapter start="6053" title="Closed source and the tldraw license"/>
        <psc:chapter start="6515" title="Open source isn't dead, it's changing"/>
        <psc:chapter start="6695" title="Wrapping up"/>
        <psc:chapter start="6763" title="Closing thoughts and stuff"/>
      </psc:chapters>
      <podcast:socialInteract protocol="activitypub" uri="https://changelog.social/@changelog/117209400941424910"/>
      <content:encoded><![CDATA[<p>This week I’m joined by Peer Richelsen, co-founder of Cal.com. What if the majority of open source repositories are already compromised and we just don’t know it yet? That’s the theory Peer brings to the table this week. We dig into how AI has flattened the knowledge graph to the point that a 16-year-old can vibe hack a power station just as easily as their mom can vibe code an iOS app, why the reporting culture that has kept open source safe all these years is collapsing under AI generated noise, Cal.com’s move to fork its own codebase and take the sensitive parts private, and the eye opening reality that shipping “$1 of AI tokens for pennies on the dollar” is now a common startup business model.</p>
<p><a href="https://changelog.zulipchat.com/#narrow/stream/456187-interviews">Join the discussion</a></p><p><a href="https://changelog.com/++" rel="payment">Changelog++</a> members save 10 minutes on this episode because they made the ads disappear. Join today!</p><p>Sponsors:</p><p><ul><li><a href="https://coder.com/">Coder.com</a> – Secure environments where devs and agents work in parallel. Open by design. Secure by default.
</li>
<li><a href="https://buildkite.com/">Buildkite</a> – You deserve better CI. Buildkite is engineered for frontier scale and trusted by the teams setting the pace.
</li>
<li><a href="https://workos.com">WorkOS</a> – Auth for CLI with AuthKit from WorkOS — Bring secure browser-based login to your terminal apps using the OAuth Device Flow, with the same polished AuthKit experience plus SSO, MFA, and passkeys. Learn more at <a href="https://workos.com/">WorkOS.com</a> and <a href="https://www.authkit.com">AuthKit.com</a>
</li>
<li><a href="https://fly.io/">Fly.io</a> – <strong>The home of Changelog.com</strong> — Deploy your apps close to your users — global Anycast load-balancing, zero-configuration private networking, hardware isolation, and instant WireGuard VPN connections. Push-button deployments that scale to thousands of instances. <a href="https://fly.io/speedrun/">Check out the speedrun</a> to get started in minutes.
</li>
</ul></p><p>Featuring:</p><ul><li>Peer Richelsen &ndash; <a href="https://cal.com/peer" rel="external ugc">Website</a>, <a href="https://github.com/PeerRich" rel="external ugc">GitHub</a>, <a href="https://www.linkedin.com/in/peer-richelsen-221233138" rel="external ugc">LinkedIn</a>, <a href="https://x.com/peer_rich" rel="external ugc">X</a></li><li>Adam Stacoviak &ndash; <a href="https://adamstacoviak.com/" rel="external ugc">Website</a>, <a href="https://github.com/adamstac" rel="external ugc">GitHub</a>, <a href="https://www.linkedin.com/in/adamstacoviak" rel="external ugc">LinkedIn</a>, <a href="https://changelog.social/@adam" rel="external ugc">Mastodon</a>, <a href="https://x.com/adamstac" rel="external ugc">X</a></li></ul></p><p>Show Notes:</p><p><p>Editorial disclosure: Adam states in the episode that he is a small seed investor in Cal.com.</p>
<h4>Cal.com and Cal.diy</h4>
<ul>
<li><a href="https://cal.com/blog/cal-com-goes-closed-source-why">Cal.com is going closed source — here’s why</a></li>
<li><a href="https://cal.com/blog/cal-diy-open-source-to-closed-source">Moving to closed-source: the technical changes</a></li>
<li><a href="https://github.com/calcom/cal.diy">Cal.diy on GitHub</a></li>
<li><a href="https://github.com/calcom/cal.diy/blob/main/CONTRIBUTING.md">Cal.diy contributing guide</a></li>
<li><a href="https://cal.com/">Cal.com</a></li>
</ul>
<h4>Open source, agents, and contribution workflows</h4>
<ul>
<li><a href="https://swamp-club.com/">Swamp Club</a></li>
<li><a href="https://openclaw.ai/">OpenClaw</a></li>
<li><a href="https://mitchellh.com/writing/non-trivial-vibing">Mitchell Hashimoto: Vibing a Non-Trivial Ghostty Feature</a></li>
<li><a href="https://github.blog/ai-and-ml/automate-repository-tasks-with-github-agentic-workflows/">GitHub Agentic Workflows</a></li>
<li><a href="https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/manage-rationale-confidence-approvals">GitHub issue-intent, rationale, confidence, and approvals</a></li>
</ul>
<h4>AI-assisted security</h4>
<ul>
<li><a href="https://blog.mozilla.org/en/firefox/hardening-firefox-anthropic-red-team/">Mozilla: Hardening Firefox with Anthropic’s Red Team</a></li>
<li><a href="https://blog.mozilla.org/en/firefox/ai-security-zero-day-vulnerabilities/">Mozilla: The zero-days are numbered</a></li>
<li><a href="https://www.anthropic.com/news/mozilla-firefox-security">Anthropic and Mozilla’s Firefox security collaboration</a></li>
<li><a href="https://nextjs.org/blog">Next.js security advisories</a></li>
<li><a href="https://github.com/BerriAI/litellm/issues/24512">LiteLLM issue: malicious package and credential stealer</a></li>
</ul>
</p><p>Something missing or broken? <a href="https://github.com/thechangelog/show-notes/blob/master/podcast/the-changelog-685.md">PRs welcome!</a></p>]]></content:encoded>
    </item></channel></rss>"####;

    // 真实 .torrent 文件的原始字节：
    // https://archive.org/download/100200300_359/100200300_359_archive.torrent
    const TORRENT_BYTES: &[u8] = b"d8:announce36:http://bt1.archive.org:6969/announce13:announce-listll36:http://bt1.archive.org:6969/announceel36:http://bt2.archive.org:6969/announceee7:comment610:This content hosted at the Internet Archive at https://archive.org/details/100200300_359\x0aFiles may have changed, which prevents torrents from downloading correctly or completely; please check for an updated torrent at https://archive.org/download/100200300_359/100200300_359_archive.torrent\x0aNote: retrieval usually requires a client that supports webseeding (GetRight style).\x0aNote: many Internet Archive torrents contain a 'pad file' directory. This directory and the files within it may be erased once retrieval completes.\x0aNote: the file 100200300_359_meta.xml contains metadata about this torrent's contents.10:created by15:ia_make_torrent13:creation datei1789754339e4:infod11:collectionsl25:org.archive.100200300_359e5:filesld5:crc328:63149d046:lengthi852e3:md532:07087cf86f3efc0ee1c0fafaf50cd8715:mtime10:16130920224:pathl22:100200300_359_meta.xmle4:sha140:bea5afc435f84ae69c41069645644564137175b4ed5:crc328:a07031ca6:lengthi1110e3:md532:50f92d705fb20a5282769454cb70a1215:mtime10:13574016134:pathl3:txte4:sha140:64c1d6f552594b0a21d9c72880182e7d3922c43fee4:name13:100200300_35912:piece lengthi524288e6:pieces20:A|\xd9A\xc5%\x8dF6.{\xc4\xb6e\xef\x12z\xe7\x10Oe6:locale2:en5:title13:100200300_3598:url-listl29:https://archive.org/download/40:http://ia902908.us.archive.org/20/items/40:http://ia802908.us.archive.org/20/items/ee";

    // 上面真实 torrent 的 info 段 SHA-1（即 info_hash）
    const TORRENT_INFO_HASH: &str = "cf8cd6ac7f30ed262ddc42009aec0d530e6cf71f";

    // 上面真实 info_hash 的 Base32 编码（RFC4648），用于构造 base32 形式的磁力链接
    const TORRENT_INFO_HASH_BASE32: &str = "Z6GNNLD7GDWSMLO4IIAJV3ANKMHGZ5Y7";

    // 各频道 <link> 的 sha1(source_key) 预期值
    const MIKAN_SOURCE_KEY: &str = "ece9fca094eecf1d43fae747de0304129b359ad0";
    const NYAA_SOURCE_KEY: &str = "d4740a133fefda70d02b1c39becf83e0675b2a9e";
    const PLAIN_SOURCE_KEY: &str = "f8d7e71fea78f3de0a0718f3146503df65075ab9";

    // 生产环境的合集过滤规则会命中 "Season 2 - 01" 这类片段，
    // 因此下面两条真实数据里只有一条能通过 parse_feed 的过滤。

    #[test]
    fn test_parse_feed_mikan_real_rss() {
        let parsed = parse_feed(MIKAN_XML.as_bytes(), "https://mikanani.me/RSS/Classic")
            .expect("real Mikan RSS should parse");

        assert_eq!(parsed.source_key, MIKAN_SOURCE_KEY);

        // 首条标题包含 "Season 2 - 01"，命中合集过滤规则后被跳过
        assert_eq!(parsed.items.len(), 1);

        let item = &parsed.items[0];
        assert_eq!(
            item.title,
            "[绿茶字幕组] FX战士久留美 / FX Senshi Kurumi-chan [01v2][WebRip][1080p][繁日内嵌]"
        );
        assert_eq!(
            item.source_url,
            "https://mikanani.me/Home/Episode/73d2014cf6522cfb9f372eec4fa8ed889076dc3c"
        );
        assert_eq!(
            item.resource_url,
            "https://mikanani.me/Download/20261002/73d2014cf6522cfb9f372eec4fa8ed889076dc3c.torrent"
        );
        // pubDate 藏在 <torrent> 扩展里，只能由 MikanParser 解析出来
        assert_eq!(item.published_at, 1_790_958_821);
        // 非磁力链接的资源，info_hash 先置零，后续再下载 torrent 计算
        assert_eq!(item.info_hash, [0_u8; 20]);
    }

    #[test]
    fn test_mikan_preprocess_xml_injects_namespace() {
        let parser = get_feed_parser("https://mikanani.me/RSS/Classic");
        let patched = parser.preprocess_xml(MIKAN_XML.as_bytes());

        // Mikan 去掉了 mikan 前缀，解析前必须补回命名空间
        assert!(patched.contains("xmlns:mikan=\"https://mikanani.me/0.1/\""));
        assert!(patched.contains("<mikan:torrent>"));
        assert!(patched.contains("</mikan:torrent>"));
        assert!(!patched.contains("<torrent xmlns="));
        assert!(!patched.contains("</torrent>"));
    }

    #[test]
    fn test_parse_feed_nyaa_real_rss() {
        let parsed = parse_feed(NYAA_XML.as_bytes(), "https://nyaa.si/?page=rss")
            .expect("real Nyaa RSS should parse");

        assert_eq!(parsed.source_key, NYAA_SOURCE_KEY);

        // 第二条标题包含 "Season 2 - 01"，命中合集过滤规则后被跳过
        assert_eq!(parsed.items.len(), 1);

        let item = &parsed.items[0];
        assert_eq!(
            item.title,
            "[Doomdos] - 트릭컬 만화동산 - 01 [1080P LFTLNET WEB-DL]"
        );
        // Nyaa 的 source_url 取 guid，resource_url 取 link
        assert_eq!(item.source_url, "https://nyaa.si/view/2168588");
        assert_eq!(item.resource_url, "https://nyaa.si/download/2168588.torrent");
        assert_eq!(item.published_at, 1_790_931_226);
        // nyaa:infoHash 扩展直接给出 info_hash
        assert_eq!(
            hex::encode(item.info_hash),
            "d18e3d4491ddaee333cc23786ea264c1eac225cc"
        );
    }

    #[test]
    fn test_parse_feed_plain_rss() {
        let parsed = parse_feed(PLAIN_RSS.as_bytes(), "https://changelog.com/podcast/feed")
            .expect("real plain RSS should parse");

        assert_eq!(parsed.source_key, PLAIN_SOURCE_KEY);
        assert_eq!(parsed.items.len(), 1);

        let item = &parsed.items[0];
        assert_eq!(item.title, "Forking Cal.com to closed source (Interview)");
        // DefaultParser 的 source_url 就是 item 的 link
        assert_eq!(item.source_url, "https://changelog.com/podcast/685");
        assert_eq!(
            item.resource_url,
            "https://op3.dev/e/https://pscrb.fm/rss/p/https://cdn.changelog.com/uploads/podcast/685/the-changelog-685.mp3"
        );
        assert_eq!(item.published_at, 1_788_465_600);
        assert_eq!(item.info_hash, [0_u8; 20]);
    }

    #[test]
    fn test_parse_feed_malformed_xml_returns_error() {
        let err = parse_feed(b"<rss version=\"2.0\"><channel>", "https://example.com/feed")
            .err()
            .expect("truncated XML should return Err");
        assert!(matches!(err, FeedFetchError::InvalidData(_)));
    }

    #[test]
    fn test_parse_feed_empty_channel_title_returns_error() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>   </title><link>https://example.com/feed</link></channel></rss>"#;
        let err = parse_feed(xml.as_bytes(), "https://example.com/feed")
            .err()
            .expect("missing channel title should return Err");
        assert!(matches!(err, FeedFetchError::InvalidData(_)));
        assert!(err.to_string().contains("missing title"));
    }

    #[test]
    fn test_parse_feed_empty_channel_link_returns_error() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>测试频道</title><link></link></channel></rss>"#;
        let err = parse_feed(xml.as_bytes(), "https://example.com/feed")
            .err()
            .expect("missing channel link should return Err");
        assert!(matches!(err, FeedFetchError::InvalidData(_)));
        assert!(err.to_string().contains("missing link"));
    }

    #[test]
    fn test_is_collection_resource_matches_collection_titles() {
        // 以下均为 Mikan 真实条目
        assert!(is_collection_resource(
            "[LoliHouse] 入间同学入魔了！S4 / Mairimashita! Iruma-kun S4 [01-24 合集][WebRip 1080p HEVC-10bit AAC][简繁内封字幕][Fin]"
        ));
        assert!(is_collection_resource(
            "[LoliHouse] 盗掘王(韩语版) / 盗墓王 / 我独自盗墓 / Tomb Raider King / Toukutsu Ou [01-12 合集][WebRip 1080p HEVC-10bit AAC][无字幕][Fin]"
        ));
        assert!(is_collection_resource("Foo (01-12 END)"));
        assert!(is_collection_resource("Foo - 01-12 END"));
    }

    #[test]
    fn test_is_collection_resource_rejects_single_episode_titles() {
        assert!(!is_collection_resource(
            "[绿茶字幕组] FX战士久留美 / FX Senshi Kurumi-chan [01v2][WebRip][1080p][繁日内嵌]"
        ));
        assert!(!is_collection_resource(
            "[Doomdos] - 트릭컬 만화동산 - 01 [1080P LFTLNET WEB-DL]"
        ));
        // "12 END" 没有集数区间，不能误判为合集
        assert!(!is_collection_resource("Foo - 12 END"));
        assert!(!is_collection_resource("Forking Cal.com to closed source (Interview)"));
    }

    #[test]
    fn test_build_source_key_normalizes_and_hashes() {
        // 与 sha1("https://mikanani.me/rss/classic") 一致
        let expected = "74e17ca5d9a3e95f948f76d935db60efd54a86ae";

        assert_eq!(build_source_key("https://mikanani.me/rss/classic"), expected);
        // 首尾空白与大小写必须被归一化
        assert_eq!(build_source_key("  HTTPS://Mikanani.ME/RSS/Classic  "), expected);
        assert_eq!(build_source_key("https://mikanani.me/rss/classic").len(), 40);
    }

    #[test]
    fn test_magnet_info_hash_parses_hex_and_base32() {
        let expected = torrent_info_hash(TORRENT_BYTES).expect("real torrent should compute info_hash");

        let hex_magnet = format!("magnet:?xt=urn:btih:{TORRENT_INFO_HASH}");
        assert_eq!(
            magnet_info_hash(&hex_magnet).expect("hex magnet should parse"),
            Some(expected)
        );

        // 附带其它参数时仍然只取 xt
        let hex_magnet_with_dn = format!("magnet:?xt=urn:btih:{TORRENT_INFO_HASH}&dn=test");
        assert_eq!(
            magnet_info_hash(&hex_magnet_with_dn).expect("magnet with dn should parse"),
            Some(expected)
        );

        let base32_magnet = format!("magnet:?xt=urn:btih:{TORRENT_INFO_HASH_BASE32}");
        assert_eq!(
            magnet_info_hash(&base32_magnet).expect("base32 magnet should parse"),
            Some(expected)
        );
        assert_eq!(
            hex::encode(expected),
            TORRENT_INFO_HASH,
            "base32 and hex must yield the same info_hash"
        );
    }

    #[test]
    fn test_magnet_info_hash_returns_none_for_non_magnet() {
        // 非 magnet 协议
        assert_eq!(
            magnet_info_hash("https://nyaa.si/download/2168588.torrent")
                .expect("non magnet link should not fail"),
            None
        );
        // 无法解析的字符串
        assert_eq!(
            magnet_info_hash("not a url").expect("unparsable link should not fail"),
            None
        );
        // magnet 缺少 xt 参数
        assert_eq!(
            magnet_info_hash("magnet:?dn=test").expect("missing xt should not fail"),
            None
        );
        // xt 不是 btih
        assert_eq!(
            magnet_info_hash(&format!("magnet:?xt=urn:sha1:{TORRENT_INFO_HASH}"))
                .expect("non btih xt should not fail"),
            None
        );
    }

    #[test]
    fn test_magnet_info_hash_rejects_invalid_btih() {
        // 长度合法但 hex 非法
        let bad_hex = format!("magnet:?xt=urn:btih:{}", "z".repeat(40));
        assert!(magnet_info_hash(&bad_hex).is_err());

        // base32 解出的字节数不是 20
        let bad_base32 = "magnet:?xt=urn:btih:AAAAAAAAAAAAAAAA";
        assert!(magnet_info_hash(bad_base32).is_err());
    }

    #[test]
    fn test_torrent_info_hash_on_real_torrent() {
        let hash = torrent_info_hash(TORRENT_BYTES).expect("real torrent should parse");
        assert_eq!(hex::encode(hash), TORRENT_INFO_HASH);
    }

    #[test]
    fn test_torrent_info_hash_rejects_bytes_missing_info() {
        // 合法的 bencode，但没有 info 段
        assert!(torrent_info_hash(b"d4:spam4:eggse").is_err());
        // 空字节
        assert!(torrent_info_hash(b"").is_err());
        // 非法 bencode
        assert!(torrent_info_hash(b"not bencode at all").is_err());
    }

    #[test]
    fn test_get_feed_parser_selects_parser_by_url() {
        // Mikan：只有 MikanParser 会改写命名空间
        let mikan_parser = get_feed_parser("https://mikanani.me/RSS/Classic");
        let patched = mikan_parser.preprocess_xml(MIKAN_XML.as_bytes());
        assert!(patched.contains("<mikan:torrent>"));

        // Mikan 的 pubDate 在 <mikan:torrent> 内，DefaultParser 取不到
        let mikan_channel =
            Channel::read_from(Cursor::new(patched.as_bytes())).expect("Mikan XML should parse");
        let mikan_item = &mikan_channel.items()[0];
        assert_eq!(mikan_parser.extract_pub_date(mikan_item), Some(1_790_959_920));
        assert_eq!(DefaultParser.extract_pub_date(mikan_item), None);

        // Nyaa：source_url 取 guid，resource_url 取 link
        let nyaa_parser = get_feed_parser("https://nyaa.si/?page=rss");
        let nyaa_channel =
            Channel::read_from(Cursor::new(NYAA_XML.as_bytes())).expect("Nyaa XML should parse");
        let nyaa_item = &nyaa_channel.items()[0];
        assert_eq!(
            nyaa_parser.extract_source_url(nyaa_item),
            Some("https://nyaa.si/view/2168588".to_string())
        );
        assert_eq!(
            nyaa_parser.extract_resource_url(nyaa_item),
            Some("https://nyaa.si/download/2168588.torrent".to_string())
        );
        assert_eq!(
            DefaultParser.extract_source_url(nyaa_item),
            Some("https://nyaa.si/download/2168588.torrent".to_string())
        );

        // 普通 RSS：DefaultParser 的 source_url 就是 link
        let default_parser = get_feed_parser("https://changelog.com/podcast/feed");
        let plain_channel =
            Channel::read_from(Cursor::new(PLAIN_RSS.as_bytes())).expect("plain RSS should parse");
        let plain_item = &plain_channel.items()[0];
        assert_eq!(
            default_parser.extract_source_url(plain_item),
            Some("https://changelog.com/podcast/685".to_string())
        );
    }

    #[test]
    fn test_handle_status_error_mapping() {
        // 明确的客户端/服务端错误视为不可达
        assert!(matches!(
            HttpFeedFetcher::handle_status_error("https://example.com", reqwest::StatusCode::NOT_FOUND),
            FeedFetchError::Inaccessible(_)
        ));
        assert!(matches!(
            HttpFeedFetcher::handle_status_error(
                "https://example.com",
                reqwest::StatusCode::UNAUTHORIZED
            ),
            FeedFetchError::Inaccessible(_)
        ));
        // 其它状态码视为可重试
        assert!(matches!(
            HttpFeedFetcher::handle_status_error(
                "https://example.com",
                reqwest::StatusCode::SERVICE_UNAVAILABLE
            ),
            FeedFetchError::Retryable(_)
        ));
        assert!(matches!(
            HttpFeedFetcher::handle_status_error(
                "https://example.com",
                reqwest::StatusCode::TOO_MANY_REQUESTS
            ),
            FeedFetchError::Retryable(_)
        ));
    }
}
