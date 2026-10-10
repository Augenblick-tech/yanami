use anyhow::Result;
use futures::Stream;
use std::pin::Pin;

use async_trait::async_trait;

use crate::entity::model::{ResourceBaseData, ResourceProp, ResourceQuery};

#[async_trait]
pub trait ResourceRepository: Send + Sync {
    fn stream<'a>(
        &'a self,
        query: &'a ResourceQuery,
    ) -> Pin<Box<dyn Stream<Item = Result<ResourceProp>> + Send + 'a>>;

    async fn insert_or_skip(&self, items: Vec<ResourceBaseData>) -> Result<()>;
    async fn insert_or_skip_return_new(
        &self,
        items: Vec<ResourceBaseData>,
    ) -> Result<Vec<ResourceProp>>;

    /// 按 info_hash 读取资源，库中不存在的 info_hash 不会出现在结果里
    async fn find_by_info_hashes(&self, info_hashes: &[[u8; 20]]) -> Result<Vec<ResourceProp>>;
}
