use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedData {
    pub source_key: String,
    pub items: Vec<FeedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub title: String,
    pub source_url: String,
    pub resource_url: String,
    pub published_at: i64,
    pub info_hash: [u8; 20],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedProp {
    pub data: FeedBaseData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedBaseData {
    pub id: i64,
    pub metadata: FeedMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedMetadata {
    pub title: String,
    pub site_url: Option<String>,
    pub search_url: Option<String>,
    pub source_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedType {
    Site,
    Search,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedListQuery {
    pub feed_type: FeedType,
}

#[derive(Debug, Error)]
pub enum FeedFetchError {
    /// 目标不可达
    #[error("rss resource not accessible: {0}")]
    Inaccessible(String),

    /// 目标临时性错误
    #[error("rss request failed: {0}")]
    Retryable(String),

    /// 不可恢复错误
    #[error("rss data corrupted or invalid: {0}")]
    InvalidData(String),
}

#[derive(Debug)]
pub enum FeedFetchResult {
    Success(Vec<FeedItem>),
    Retryable(common::shared::error::Error),
    Failure(common::shared::error::Error),
    Denied,
}
