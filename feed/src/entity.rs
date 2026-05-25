use std::{collections::HashSet, sync::Arc, sync::OnceLock};

use async_trait::async_trait;
use domain::{
    feed::{FeedSource, FeedSourceId},
    shared::error::DomainError,
};

use regex::Regex;

use crate::contracts::{FeedData, FeedFetcher, FeedSourceKeyUpdater};

#[path = "validate.rs"]
mod validate;

#[derive(Clone)]
pub struct FeedEntity {
    source: FeedSource,
    fetcher: Arc<dyn FeedFetcher>,
    source_key_updater: Arc<dyn FeedSourceKeyUpdater>,
}

impl FeedEntity {
    pub fn new(source: FeedSource, fetcher: Arc<dyn FeedFetcher>) -> Result<Self, DomainError> {
        Self::new_with_source_key_updater(source, fetcher, Arc::new(NoopFeedSourceKeyUpdater))
    }

    pub(crate) fn new_with_source_key_updater(
        source: FeedSource,
        fetcher: Arc<dyn FeedFetcher>,
        source_key_updater: Arc<dyn FeedSourceKeyUpdater>,
    ) -> Result<Self, DomainError> {
        validate_source(&source)?;
        Ok(Self {
            source,
            fetcher,
            source_key_updater,
        })
    }

    pub fn read_data(&self) -> &FeedSource {
        &self.source
    }

    pub fn into_snapshot(self) -> FeedSource {
        self.source
    }

    pub async fn fetch(&mut self) -> Result<FeedData, DomainError> {
        let data = self.fetcher.fetch(&self.source).await?;
        self.update_source_key_from_feed_data(&data).await?;
        Ok(data)
    }

    pub async fn search(&mut self, keyword: &str) -> Result<FeedData, DomainError> {
        let data = self.fetcher.search(&self.source, keyword).await?;
        self.update_source_key_from_feed_data(&data).await?;
        Ok(data)
    }

    /// 判断标题是否为合集包，合集包不应进入资源处理流程。
    pub fn is_collection_pack(title: &str) -> bool {
        if title.contains("合集") {
            return true;
        }
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        let re = PATTERN.get_or_init(|| {
            Regex::new(r"\[\d+\s*-\s*\d+\]").expect("invalid collection pack regex")
        });
        re.is_match(title)
    }

    async fn update_source_key_from_feed_data(
        &mut self,
        data: &FeedData,
    ) -> Result<(), DomainError> {
        let source_key = data.source_key.trim();
        if source_key.is_empty() {
            return Err(DomainError::InvariantViolation(
                "feed source key cannot be empty",
            ));
        }
        if self
            .source
            .source_key
            .as_deref()
            .is_some_and(|current| current == source_key)
        {
            return Ok(());
        }

        self.source_key_updater
            .update_source_key(&self.source.id, source_key)
            .await?;
        self.source.source_key = Some(source_key.to_string());
        Ok(())
    }
}

struct NoopFeedSourceKeyUpdater;

#[async_trait]
impl FeedSourceKeyUpdater for NoopFeedSourceKeyUpdater {
    async fn update_source_key(
        &self,
        _source_id: &FeedSourceId,
        _source_key: &str,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

pub(crate) fn validate_source_set(sources: &[FeedSource]) -> Result<(), DomainError> {
    let mut seen_ids = HashSet::new();
    let mut seen_sites = HashSet::new();
    let mut seen_searches = HashSet::new();
    let mut seen_source_keys = HashSet::new();

    for source in sources {
        validate_source(source)?;
        if !seen_ids.insert(source.id.0.clone()) {
            return Err(DomainError::InvariantViolation(
                "feed source id must be unique",
            ));
        }
        if let Some(site_url) = source.site_url.as_deref().map(str::trim) {
            if !site_url.is_empty() && !seen_sites.insert(site_url.to_string()) {
                return Err(DomainError::InvariantViolation(
                    "feed source site url must be unique",
                ));
            }
        }
        if let Some(search_url) = source.search_url.as_deref().map(str::trim) {
            if !search_url.is_empty() && !seen_searches.insert(search_url.to_string()) {
                return Err(DomainError::InvariantViolation(
                    "feed source search url must be unique",
                ));
            }
        }
        if let Some(source_key) = source.source_key.as_deref().map(str::trim) {
            if !source_key.is_empty() && !seen_source_keys.insert(source_key.to_string()) {
                return Err(DomainError::InvariantViolation(
                    "feed source key must be unique",
                ));
            }
        }
    }

    Ok(())
}

fn validate_source(source: &FeedSource) -> Result<(), DomainError> {
    if source.id.0.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "feed source id cannot be empty",
        ));
    }
    if source.title.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "feed source title cannot be empty",
        ));
    }

    if source
        .source_key
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(DomainError::InvariantViolation(
            "feed source key cannot be empty",
        ));
    }

    let has_site_url = source
        .site_url
        .as_deref()
        .is_some_and(|u| !u.trim().is_empty());
    let has_search_url = source
        .search_url
        .as_deref()
        .is_some_and(|u| !u.trim().is_empty());

    if !has_site_url && !has_search_url {
        return Err(DomainError::InvariantViolation(
            "feed source must have at least one of site url or search url",
        ));
    }

    if has_search_url {
        let search_url = source.search_url.as_deref().ok_or_else(|| {
            DomainError::InvariantViolation("search feed source must have search url")
        })?;
        if !search_url.contains("{}") {
            return Err(DomainError::InvariantViolation(
                "search template feed source must contain placeholder",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use domain::feed::FeedSourceId;

    use super::*;

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

    fn feed(id: &str, site_url: &str) -> FeedSource {
        FeedSource {
            id: FeedSourceId(id.to_string()),
            title: id.to_string(),
            site_url: Some(site_url.to_string()),
            search_url: None,
            source_key: None,
        }
    }

    #[test]
    fn new_accepts_valid_source() {
        let entity =
            FeedEntity::new(feed("a", "https://a.example/rss"), fetcher()).expect("entity");

        assert_eq!(entity.read_data().id.0, "a");
    }

    #[tokio::test]
    async fn fetch_does_not_require_persisted_source_key() {
        let mut entity =
            FeedEntity::new(feed("a", "https://a.example/rss"), fetcher()).expect("entity");

        let data = entity.fetch().await.expect("fetch");

        assert_eq!(data.source_key, "source-key");
    }

    #[test]
    fn source_set_validation_rejects_duplicate_search_url() {
        let error = validate_source_set(&[
            FeedSource {
                id: FeedSourceId("a".to_string()),
                title: "A".to_string(),
                site_url: None,
                search_url: Some("https://x.example/rss?keyword={}".to_string()),
                source_key: None,
            },
            FeedSource {
                id: FeedSourceId("b".to_string()),
                title: "B".to_string(),
                site_url: None,
                search_url: Some("https://x.example/rss?keyword={}".to_string()),
                source_key: None,
            },
        ])
        .expect_err("duplicate search url");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: feed source search url must be unique"
        );
    }

    #[test]
    fn source_set_validation_accepts_combined_site_and_search_source() {
        validate_source_set(&[FeedSource {
            id: FeedSourceId("dmhy".to_string()),
            title: "DMHY".to_string(),
            site_url: Some("https://share.dmhy.org/topics/rss/rss.xml".to_string()),
            search_url: Some("https://share.dmhy.org/topics/rss/rss.xml?keyword={}".to_string()),
            source_key: Some("dmhy-source".to_string()),
        }])
        .expect("same source can provide site and search capabilities");
    }
}
