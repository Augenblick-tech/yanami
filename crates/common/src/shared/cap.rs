use std::path::PathBuf;

use async_trait::async_trait;

use crate::shared::{error::Error, model::SearchUrls};

pub trait FeedSearchUrlProvider: Send + Sync {
    fn made_search_url(&self, keywords: &[String]) -> Vec<SearchUrls>;
}

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool, Error>;
}

/// 番剧季度在媒体库里的落点。
///
/// 落点由系列决定（系列目录 / 季号），需要落点的一方带着实现进来，
/// 不由消费方自己拼路径。
#[async_trait]
pub trait SeriesLocation: Send + Sync {
    async fn location_of(&self, anime_id: i64) -> Result<PathBuf, Error>;
}
