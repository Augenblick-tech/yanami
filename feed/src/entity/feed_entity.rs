use domain::shared::error::DomainError;

use crate::entity::cap::{FeedFetcher, FeedItem, FeedUpdater};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntity {
    pub id: String,
    pub title: String,
    pub site_url: Option<String>,
    pub search_url: Option<String>,
    pub source_key: Option<String>,
}

impl FeedEntity {
    pub(crate) fn new(
        id: String,
        title: String,
        site_url: Option<String>,
        search_url: Option<String>,
        source_key: Option<String>,
    ) -> Result<Self, DomainError> {
        let entity = Self {
            id,
            title,
            site_url,
            search_url,
            source_key,
        };
        validate_source(&entity)?;
        Ok(entity)
    }

    pub async fn verify(&mut self, fetch_cap: &dyn FeedFetcher) -> Result<(), DomainError> {
        let mut source_key = None;
        if let Some(url) = &self.site_url {
            let data = fetch_cap.fetch_url(url).await?;
            source_key = Some(data.source_key);
        }

        if let Some(url) = &self.search_url {
            let url = formatx::formatx!(url, "败犬")
                .map_err(|e| DomainError::external("format search url failed", e))?;
            let data = fetch_cap.fetch_url(&url).await?;
            let key = data.source_key;
            if let Some(source_key) = &source_key {
                if source_key != &key {
                    return Err(DomainError::InvariantViolation(
                        "feed verify failed, site url and search url source must be same",
                    ));
                }
            } else {
                source_key = Some(key);
            }
        }
        if &self.source_key != &source_key {
            self.source_key = source_key;
        }
        Ok(())
    }

    pub async fn list(
        &mut self,
        fetch_cap: &dyn FeedFetcher,
        update_cap: &dyn FeedUpdater,
    ) -> Result<Vec<FeedItem>, DomainError> {
        if let Some(url) = &self.site_url {
            let data = fetch_cap.fetch_url(url).await?;
            self.set_source_key(data.source_key, update_cap).await?;
            Ok(data.items)
        } else {
            Err(DomainError::InvariantViolation("not found feed site url"))
        }
    }

    pub async fn search(
        &mut self,
        keyword: &str,
        cap: &dyn FeedFetcher,
        update_cap: &dyn FeedUpdater,
    ) -> Result<Vec<FeedItem>, DomainError> {
        if let Some(url) = &self.search_url {
            let url = formatx::formatx!(url, keyword)
                .map_err(|e| DomainError::external("format search url failed", e))?;
            let data = cap.fetch_url(&url).await?;
            self.set_source_key(data.source_key, update_cap).await?;
            Ok(data.items)
        } else {
            Err(DomainError::InvariantViolation("not found feed search url"))
        }
    }

    async fn set_source_key(
        &mut self,
        source_key: String,
        update_cap: &dyn FeedUpdater,
    ) -> Result<(), DomainError> {
        if let Some(key) = &self.source_key {
            if key != &source_key {
                // 如果存在source_key，一次更新失败，可以忽略
                if update_cap
                    .set_source_key(&self.id, &source_key)
                    .await
                    .is_ok()
                {
                    self.source_key = Some(source_key);
                }
            }
        } else {
            // 如果不存在source_key，更新失败不能返回数据
            update_cap.set_source_key(&self.id, &source_key).await?;
            self.source_key = Some(source_key);
        }
        Ok(())
    }
}

fn validate_source(source: &FeedEntity) -> Result<(), DomainError> {
    if source.id.trim().is_empty() {
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
