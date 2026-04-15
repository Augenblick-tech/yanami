use async_trait::async_trait;

use crate::shared::error::DomainError;

pub struct FeedSourceUpdate {
    pub title: String,
    pub site_url: Option<String>,
    pub search_url: Option<String>,
}

#[async_trait]
pub trait FeedSourceWriterCap: Send + Sync {
    async fn write_source(
        &self,
        scope: (&str, i64),
        source: &FeedSourceUpdate,
    ) -> Result<(), DomainError>;
}
