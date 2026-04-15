use domain::{feed::FeedSource, shared::error::DomainError};

use crate::entity::FeedEntity;

impl FeedEntity {
    pub fn replace_source(&mut self, source: FeedSource) -> Result<(), DomainError> {
        validate_source(&source)?;
        self.source = source;
        Ok(())
    }
}

use crate::entity::validate_source;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use domain::feed::FeedSourceId;

    use super::*;
    use crate::contracts::{FeedData, FeedFetcher};

    struct NoopFeedFetcher;

    #[async_trait]
    impl FeedFetcher for NoopFeedFetcher {
        async fn fetch(&self, source: &FeedSource) -> Result<FeedData, DomainError> {
            Ok(FeedData {
                source_key: source
                    .source_key
                    .clone()
                    .unwrap_or_else(|| "source-key".to_string()),
                items: vec![],
            })
        }

        async fn search(
            &self,
            source: &FeedSource,
            _keyword: &str,
        ) -> Result<FeedData, DomainError> {
            Ok(FeedData {
                source_key: source
                    .source_key
                    .clone()
                    .unwrap_or_else(|| "source-key".to_string()),
                items: vec![],
            })
        }
    }

    fn fetcher() -> Arc<dyn FeedFetcher> {
        Arc::new(NoopFeedFetcher)
    }

    fn feed(id: &str, url: &str) -> FeedSource {
        FeedSource {
            id: FeedSourceId(id.to_string()),
            title: id.to_string(),
            site_url: Some(url.to_string()),
            search_url: None,
            source_key: None,
        }
    }

    #[test]
    fn replace_source_accepts_valid_source() {
        let mut entity =
            FeedEntity::new(feed("a", "https://a.example/rss"), fetcher()).expect("entity");

        entity
            .replace_source(feed("b", "https://b.example/rss"))
            .expect("replace source");

        assert_eq!(entity.read_data().id.0, "b");
    }

    #[test]
    fn replace_source_keeps_original_when_validation_fails() {
        let mut entity =
            FeedEntity::new(feed("a", "https://a.example/rss"), fetcher()).expect("entity");

        let error = entity
            .replace_source(feed("b", ""))
            .expect_err("empty site url must fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: feed source must have at least one of site url or search url"
        );
        assert_eq!(entity.read_data().id.0, "a");
    }
}
