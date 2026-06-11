use anyhow::Result;
use async_trait::async_trait;

use crate::entity::model::{
    FeedBaseData, FeedData, FeedFetchError, FeedFetchResult, FeedListQuery, FeedMetadata, FeedProp,
};

#[async_trait]
pub trait FeedFetcher: Send + Sync {
    async fn fetch_url(&self, url: &str) -> Result<FeedData, FeedFetchError>;
    async fn get_source_key(&self, url: &str) -> Result<String, FeedFetchError>;
}

#[async_trait]
pub trait FeedRepository: Send + Sync {
    async fn list(&self, query: &FeedListQuery) -> Result<Vec<FeedProp>>;
    async fn insert(&self, entity: &FeedMetadata) -> Result<FeedProp>;
    async fn update(&self, entity: &FeedBaseData) -> Result<()>;
    async fn delete(&self, id: i64) -> Result<()>;
    async fn get(&self, id: i64) -> Result<Option<FeedProp>>;
}

// feed访问策略
pub trait FeedAccessPolicy: Send + Sync {
    // 获取当前这一刻禁止请求的feed id列表
    fn block_feed_ids(&self) -> Vec<i64>;
    // 获取退避的feed id及截止时间戳
    fn block_feed_details(&self) -> Vec<(i64, i64)>;
    // 校验是否允许请求
    fn is_access(&self, feed_id: i64) -> bool;
    // 记录请求结果
    fn note(&self, feed_id: i64, res: &FeedFetchResult);
}
