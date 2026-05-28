use std::{collections::HashSet, sync::OnceLock};

use domain::{feed::FeedSource, shared::error::DomainError};

use regex::Regex;

use crate::contracts::{FeedData, FeedFetcher};

#[path = "validate.rs"]
mod validate;

#[derive(Debug, Clone)]
pub struct FeedEntity {
    source: FeedSource,
}

impl FeedEntity {
    pub fn new(source: FeedSource) -> Result<Self, DomainError> {
        validate_source(&source)?;
        Ok(Self { source })
    }

    pub fn read_data(&self) -> &FeedSource {
        &self.source
    }

    pub fn into_snapshot(self) -> FeedSource {
        self.source
    }

    pub async fn fetch(&mut self, fetcher: &dyn FeedFetcher) -> Result<FeedData, DomainError> {
        let url = self
            .source
            .site_url
            .as_deref()
            .ok_or(DomainError::InvariantViolation("feed has no site url"))?;
        let data = fetcher.fetch_url(url).await?;
        self.update_source_key_from_feed_data(&data);
        Ok(data)
    }

    pub async fn search(
        &mut self,
        keyword: &str,
        fetcher: &dyn FeedFetcher,
    ) -> Result<FeedData, DomainError> {
        let template = self
            .source
            .search_url
            .as_deref()
            .ok_or(DomainError::InvariantViolation("feed has no search url"))?;
        let url = template.replacen("{}", keyword, 1);
        let data = fetcher.fetch_url(&url).await?;
        self.update_source_key_from_feed_data(&data);
        Ok(data)
    }

    pub fn build_search_url(&self, keyword: &str) -> Result<String, DomainError> {
        let template = self
            .source
            .search_url
            .as_deref()
            .ok_or(DomainError::InvariantViolation("feed has no search url"))?;
        Ok(template.replacen("{}", keyword, 1))
    }

    /// 判断标题是否为合集包，合集包不应进入资源处理流程。
    pub fn is_collection_pack(title: &str) -> bool {
        if title.contains("合集") {
            return true;
        }
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        let re = PATTERN.get_or_init(|| {
            Regex::new(r"\[\d+\s*-\s*\d+\]").expect("invalid collection pack regex")
            // SAFETY: 编译期已知有效的正则字面量
        });
        re.is_match(title)
    }

    fn update_source_key_from_feed_data(&mut self, data: &FeedData) {
        let source_key = data.source_key.trim();
        if self
            .source
            .source_key
            .as_deref()
            .is_some_and(|current| current == source_key)
        {
            return;
        }
        self.source.source_key = Some(source_key.to_string());
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
        async fn fetch_url(&self, _url: &str) -> Result<FeedData, DomainError> {
            Ok(FeedData {
                source_key: "source-key".to_string(),
                items: vec![],
            })
        }
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
        let entity = FeedEntity::new(feed("a", "https://a.example/rss")).expect("entity");

        assert_eq!(entity.read_data().id.0, "a");
    }

    #[tokio::test]
    async fn fetch_does_not_require_persisted_source_key() {
        let mut entity = FeedEntity::new(feed("a", "https://a.example/rss")).expect("entity");

        let data = entity.fetch(&NoopFeedFetcher).await.expect("fetch");

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
