use async_trait::async_trait;

use crate::shared::{error::Error, model::SearchUrls};

pub trait FeedSearchUrlProvider: Send + Sync {
    fn made_search_url(&self, keywords: &[String]) -> Vec<SearchUrls>;
}

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool, Error>;
}
