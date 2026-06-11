use std::sync::Arc;

use common::shared::error::Error;

use crate::entity::{
    cap::{FeedAccessPolicy, FeedFetcher},
    model::{FeedBaseData, FeedFetchResult, FeedMetadata},
};

#[derive(Clone)]
pub struct FeedEntity {
    data: FeedBaseData,
    fetch_cap: Arc<dyn FeedFetcher>,
    access_policy: Arc<dyn FeedAccessPolicy>,
}

impl FeedEntity {
    pub(crate) fn new(
        data: FeedBaseData,
        fetch_cap: Arc<dyn FeedFetcher>,
        access_policy: Arc<dyn FeedAccessPolicy>,
    ) -> Self {
        Self {
            data,
            fetch_cap,
            access_policy,
        }
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn title(&self) -> &str {
        &self.data.metadata.title
    }

    pub fn site_url(&self) -> Option<&str> {
        self.data.metadata.site_url.as_deref()
    }

    pub fn search_url(&self) -> Option<&str> {
        self.data.metadata.search_url.as_deref()
    }

    pub async fn set(
        &mut self,
        title: String,
        site_url: Option<String>,
        search_url: Option<String>,
    ) -> Result<(), Error> {
        let metdata =
            Self::verify_metadata(self.fetch_cap.as_ref(), title, site_url, search_url).await?;
        self.data.metadata = metdata;
        Ok(())
    }

    pub async fn list(&self) -> Result<FeedFetchResult, Error> {
        if !self.access_policy.is_access(self.data.id) {
            return Ok(FeedFetchResult::Denied);
        }
        if let Some(url) = &self.data.metadata.site_url {
            let data = self.fetch_cap.fetch_url(url).await;
            let res = match data {
                Ok(data) => FeedFetchResult::Success(data.items),
                Err(e) => match e {
                    super::model::FeedFetchError::Inaccessible(v) => {
                        FeedFetchResult::Failure(Error::conflict(v))
                    }
                    super::model::FeedFetchError::Retryable(v) => {
                        FeedFetchResult::Retryable(Error::conflict(v))
                    }
                    super::model::FeedFetchError::InvalidData(v) => {
                        return Err(Error::conflict(v));
                    }
                },
            };
            self.access_policy.note(self.data.id, &res);
            Ok(res)
        } else {
            Err(Error::invariant("not found feed site url"))
        }
    }

    pub async fn search(&self, keyword: &str) -> Result<FeedFetchResult, Error> {
        if let Some(url) = &self.data.metadata.search_url {
            let url = match formatx::formatx!(url, keyword) {
                Ok(url) => url,
                Err(e) => {
                    return Err(Error::external("format search url failed", e));
                }
            };
            let data = self.fetch_cap.fetch_url(&url).await;
            let res = match data {
                Ok(data) => FeedFetchResult::Success(data.items),
                Err(e) => match e {
                    super::model::FeedFetchError::Inaccessible(v) => {
                        FeedFetchResult::Failure(Error::conflict(v))
                    }
                    super::model::FeedFetchError::Retryable(v) => {
                        FeedFetchResult::Retryable(Error::conflict(v))
                    }
                    super::model::FeedFetchError::InvalidData(v) => return Err(Error::conflict(v)),
                },
            };
            self.access_policy.note(self.data.id, &res);
            Ok(res)
        } else {
            Err(Error::invariant("not found feed site url"))
        }
    }
}

impl FeedEntity {
    pub(super) async fn verify_metadata(
        fetch_cap: &dyn FeedFetcher,
        title: String,
        site_url: Option<String>,
        search_url: Option<String>,
    ) -> Result<FeedMetadata, Error> {
        if site_url.is_none() && search_url.is_none() {
            return Err(Error::conflict("feed entity must have url"));
        }

        if title.is_empty() {
            return Err(Error::conflict("feed entity title must be not empty"));
        }

        let source_key = FeedEntity::get_source_key(fetch_cap, &site_url, &search_url).await?;
        Ok(FeedMetadata {
            title,
            site_url,
            search_url,
            source_key,
        })
    }

    pub(super) async fn get_source_key(
        fetch_cap: &dyn FeedFetcher,
        url: &Option<String>,
        search_url: &Option<String>,
    ) -> Result<String, Error> {
        let mut source_key = None;
        if let Some(url) = url {
            let data = fetch_cap
                .get_source_key(url)
                .await
                .map_err(|e| Error::external("feed verify fetch url failed", e))?;
            source_key = Some(data);
        }

        if let Some(url) = search_url {
            let url = formatx::formatx!(url, "败犬")
                .map_err(|e| Error::external("feed get_source_key format search url failed", e))?;
            let data = fetch_cap
                .get_source_key(&url)
                .await
                .map_err(|e| Error::external("feed get_source_key fetch search url failed", e))?;
            if let Some(source_key) = &source_key {
                if source_key != &data {
                    return Err(Error::invariant(
                        "feed get_source_key failed, site url and search url source must be same",
                    ));
                }
            } else {
                source_key = Some(data);
            }
        }

        if let Some(key) = source_key {
            Ok(key)
        } else {
            Err(Error::conflict("get source key failed"))
        }
    }

    pub(super) fn get_base_data(&self) -> &FeedBaseData {
        &self.data
    }
}
