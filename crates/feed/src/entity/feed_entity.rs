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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::model::{FeedData, FeedFetchError, FeedItem};
    use async_trait::async_trait;
    use std::sync::Mutex;

    const SITE_URL: &str = "https://mikanani.me/RSS/Classic";
    const SEARCH_URL: &str = "https://mikanani.me/RSS/Search?searchstr={}";

    // Mock 的抓取结果（FeedFetchError 未实现 Clone，因此用枚举描述行为）
    #[derive(Clone)]
    enum FetchOutcome {
        Data(FeedData),
        Inaccessible,
        Retryable,
        InvalidData,
    }

    // Mock 生产环境声明的 FeedFetcher trait，同时记录被调用的 url
    #[derive(Clone)]
    struct MockFetcher {
        outcome: FetchOutcome,
        keys: Vec<String>,
        key_error: bool,
        fetched_urls: Arc<Mutex<Vec<String>>>,
        key_urls: Arc<Mutex<Vec<String>>>,
    }

    impl MockFetcher {
        fn new(outcome: FetchOutcome, keys: Vec<&str>) -> Self {
            Self {
                outcome,
                keys: keys.into_iter().map(str::to_string).collect(),
                key_error: false,
                fetched_urls: Arc::new(Mutex::new(Vec::new())),
                key_urls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_key_error(mut self) -> Self {
            self.key_error = true;
            self
        }

        fn fetched_urls(&self) -> Vec<String> {
            self.fetched_urls.lock().expect("lock poisoned").clone()
        }

        fn key_urls(&self) -> Vec<String> {
            self.key_urls.lock().expect("lock poisoned").clone()
        }
    }

    #[async_trait]
    impl FeedFetcher for MockFetcher {
        async fn fetch_url(&self, url: &str) -> anyhow::Result<FeedData, FeedFetchError> {
            self.fetched_urls
                .lock()
                .expect("lock poisoned")
                .push(url.to_string());
            match &self.outcome {
                FetchOutcome::Data(data) => Ok(data.clone()),
                FetchOutcome::Inaccessible => Err(FeedFetchError::Inaccessible(
                    "mock inaccessible".to_string(),
                )),
                FetchOutcome::Retryable => {
                    Err(FeedFetchError::Retryable("mock retryable".to_string()))
                }
                FetchOutcome::InvalidData => {
                    Err(FeedFetchError::InvalidData("mock invalid data".to_string()))
                }
            }
        }

        async fn get_source_key(&self, url: &str) -> anyhow::Result<String, FeedFetchError> {
            // 记录调用顺序，并按下标返回不同的 key（用于模拟 site/search 来源不一致）
            let index = {
                let mut urls = self.key_urls.lock().expect("lock poisoned");
                urls.push(url.to_string());
                urls.len() - 1
            };

            if self.key_error {
                return Err(FeedFetchError::Retryable("mock key error".to_string()));
            }

            Ok(self
                .keys
                .get(index)
                .or_else(|| self.keys.last())
                .cloned()
                .unwrap_or_default())
        }
    }

    // Mock 生产环境声明的 FeedAccessPolicy trait，记录校验与回调
    #[derive(Clone)]
    struct MockAccessPolicy {
        allow: bool,
        checked_ids: Arc<Mutex<Vec<i64>>>,
        notes: Arc<Mutex<Vec<(i64, &'static str)>>>,
    }

    impl MockAccessPolicy {
        fn new(allow: bool) -> Self {
            Self {
                allow,
                checked_ids: Arc::new(Mutex::new(Vec::new())),
                notes: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn notes(&self) -> Vec<(i64, &'static str)> {
            self.notes.lock().expect("lock poisoned").clone()
        }

        fn checked_ids(&self) -> Vec<i64> {
            self.checked_ids.lock().expect("lock poisoned").clone()
        }
    }

    impl FeedAccessPolicy for MockAccessPolicy {
        fn block_feed_ids(&self) -> Vec<i64> {
            Vec::new()
        }

        fn block_feed_details(&self) -> Vec<(i64, i64)> {
            Vec::new()
        }

        fn is_access(&self, feed_id: i64) -> bool {
            self.checked_ids
                .lock()
                .expect("lock poisoned")
                .push(feed_id);
            self.allow
        }

        fn note(&self, feed_id: i64, res: &FeedFetchResult) {
            let tag = match res {
                FeedFetchResult::Success(_) => "success",
                FeedFetchResult::Retryable(_) => "retryable",
                FeedFetchResult::Failure(_) => "failure",
                FeedFetchResult::Denied => "denied",
            };
            self.notes
                .lock()
                .expect("lock poisoned")
                .push((feed_id, tag));
        }
    }

    fn base_data(id: i64, site_url: Option<&str>, search_url: Option<&str>) -> FeedBaseData {
        FeedBaseData {
            id,
            metadata: FeedMetadata {
                title: "测试订阅源".to_string(),
                site_url: site_url.map(str::to_string),
                search_url: search_url.map(str::to_string),
                source_key: "preset-key".to_string(),
            },
        }
    }

    fn sample_items() -> Vec<FeedItem> {
        vec![FeedItem {
            title: "第 01 话".to_string(),
            source_url:
                "https://mikanani.me/Home/Episode/d7c45419815d1af4e5542fc7edca3f7ec3cadfdf".to_string(),
            resource_url: "https://mikanani.me/Download/20261002/d7c45419815d1af4e5542fc7edca3f7ec3cadfdf.torrent".to_string(),
            published_at: 1_790_959_920,
            info_hash: [7_u8; 20],
        }]
    }

    fn empty_data() -> FeedData {
        FeedData {
            source_key: "mock-source-key".to_string(),
            items: Vec::new(),
        }
    }

    fn build_entity(
        data: FeedBaseData,
        fetcher: &MockFetcher,
        policy: &MockAccessPolicy,
    ) -> FeedEntity {
        FeedEntity::new(data, Arc::new(fetcher.clone()), Arc::new(policy.clone()))
    }

    #[tokio::test]
    async fn test_verify_metadata_requires_at_least_one_url() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let err = FeedEntity::verify_metadata(&fetcher, "标题".to_string(), None, None)
            .await
            .expect_err("empty site/search url should fail");
        assert!(err.to_string().contains("feed entity must have url"));
    }

    #[tokio::test]
    async fn test_verify_metadata_requires_non_empty_title() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let err =
            FeedEntity::verify_metadata(&fetcher, String::new(), Some(SITE_URL.to_string()), None)
                .await
                .expect_err("empty title should fail");
        assert!(
            err.to_string()
                .contains("feed entity title must be not empty")
        );
    }

    #[tokio::test]
    async fn test_verify_metadata_accepts_same_source_key() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["same", "same"]);
        let metadata = FeedEntity::verify_metadata(
            &fetcher,
            "标题".to_string(),
            Some(SITE_URL.to_string()),
            Some(SEARCH_URL.to_string()),
        )
        .await
        .expect("matching site/search sources should pass");

        assert_eq!(metadata.title, "标题");
        assert_eq!(metadata.source_key, "same");
        assert_eq!(
            fetcher.key_urls(),
            vec![
                SITE_URL.to_string(),
                "https://mikanani.me/RSS/Search?searchstr=败犬".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_verify_metadata_uses_search_url_only() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["search-key"]);
        let metadata = FeedEntity::verify_metadata(
            &fetcher,
            "标题".to_string(),
            None,
            Some(SEARCH_URL.to_string()),
        )
        .await
        .expect("search url only should pass");

        assert_eq!(metadata.source_key, "search-key");
        assert_eq!(metadata.site_url, None);
        assert_eq!(
            fetcher.key_urls(),
            vec!["https://mikanani.me/RSS/Search?searchstr=败犬".to_string()]
        );
    }

    #[tokio::test]
    async fn test_verify_metadata_rejects_mismatched_source_key() {
        let fetcher = MockFetcher::new(
            FetchOutcome::Data(empty_data()),
            vec!["site-key", "search-key"],
        );
        let err = FeedEntity::verify_metadata(
            &fetcher,
            "标题".to_string(),
            Some(SITE_URL.to_string()),
            Some(SEARCH_URL.to_string()),
        )
        .await
        .expect_err("mismatched sources should fail");

        assert!(
            err.to_string()
                .contains("site url and search url source must be same")
        );
    }

    #[tokio::test]
    async fn test_verify_metadata_maps_fetch_error_to_external_error() {
        let fetcher =
            MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]).with_key_error();
        let err = FeedEntity::verify_metadata(
            &fetcher,
            "标题".to_string(),
            Some(SITE_URL.to_string()),
            None,
        )
        .await
        .expect_err("fetching source key failure should fail");

        assert!(err.to_string().contains("feed verify fetch url failed"));
    }

    #[tokio::test]
    async fn test_list_returns_denied_when_policy_blocks() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(false));
        let entity = build_entity(base_data(42, Some(SITE_URL), None), &fetcher, &policy);

        let result = entity
            .list()
            .await
            .expect("rejected by access policy is not an error");
        assert!(matches!(result, FeedFetchResult::Denied));
        assert!(fetcher.fetched_urls().is_empty(), "no fetch when rejected");
        assert_eq!(policy.checked_ids(), vec![42]);
        assert!(policy.notes().is_empty(), "no note callback when rejected");
    }

    #[tokio::test]
    async fn test_list_without_site_url_returns_invariant_error() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(1, None, Some(SEARCH_URL)), &fetcher, &policy);

        let err = entity
            .list()
            .await
            .expect_err("missing site url should fail");
        assert!(err.to_string().contains("not found feed site url"));
    }

    #[tokio::test]
    async fn test_list_success_returns_items_and_notes_policy() {
        let items = sample_items();
        let fetcher = MockFetcher::new(
            FetchOutcome::Data(FeedData {
                source_key: "mock-source-key".to_string(),
                items: items.clone(),
            }),
            vec!["key"],
        );
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(7, Some(SITE_URL), None), &fetcher, &policy);

        let result = entity.list().await.expect("fetch should succeed");
        match result {
            FeedFetchResult::Success(got) => assert_eq!(got, items),
            other => panic!("expected Success, got {other:?}"),
        }

        assert_eq!(fetcher.fetched_urls(), vec![SITE_URL.to_string()]);
        assert_eq!(policy.notes(), vec![(7, "success")]);
    }

    #[tokio::test]
    async fn test_list_maps_retryable_error() {
        let fetcher = MockFetcher::new(FetchOutcome::Retryable, vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(7, Some(SITE_URL), None), &fetcher, &policy);

        let result = entity
            .list()
            .await
            .expect("retryable error is not an Err branch");
        match result {
            FeedFetchResult::Retryable(error) => {
                assert_eq!(error.to_string(), "conflict: mock retryable");
            }
            other => panic!("expected Retryable, got {other:?}"),
        }
        assert_eq!(policy.notes(), vec![(7, "retryable")]);
    }

    #[tokio::test]
    async fn test_list_maps_inaccessible_error_to_failure() {
        let fetcher = MockFetcher::new(FetchOutcome::Inaccessible, vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(7, Some(SITE_URL), None), &fetcher, &policy);

        let result = entity
            .list()
            .await
            .expect("inaccessible error is not an Err branch");
        match result {
            FeedFetchResult::Failure(error) => {
                assert_eq!(error.to_string(), "conflict: mock inaccessible");
            }
            other => panic!("expected Failure, got {other:?}"),
        }
        assert_eq!(policy.notes(), vec![(7, "failure")]);
    }

    #[tokio::test]
    async fn test_list_invalid_data_returns_error() {
        let fetcher = MockFetcher::new(FetchOutcome::InvalidData, vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(7, Some(SITE_URL), None), &fetcher, &policy);

        let err = entity
            .list()
            .await
            .expect_err("invalid data is unrecoverable");
        assert_eq!(err.to_string(), "conflict: mock invalid data");
        assert!(
            policy.notes().is_empty(),
            "no access policy callback on Err"
        );
    }

    #[tokio::test]
    async fn test_search_formats_keyword_into_url() {
        let items = sample_items();
        let fetcher = MockFetcher::new(
            FetchOutcome::Data(FeedData {
                source_key: "mock-source-key".to_string(),
                items: items.clone(),
            }),
            vec!["key"],
        );
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(
            base_data(7, Some(SITE_URL), Some(SEARCH_URL)),
            &fetcher,
            &policy,
        );

        let result = entity
            .search("kurumi")
            .await
            .expect("search should succeed");
        match result {
            FeedFetchResult::Success(got) => assert_eq!(got, items),
            other => panic!("expected Success, got {other:?}"),
        }
        assert_eq!(
            fetcher.fetched_urls(),
            vec!["https://mikanani.me/RSS/Search?searchstr=kurumi".to_string()]
        );
        assert_eq!(policy.notes(), vec![(7, "success")]);
    }

    #[tokio::test]
    async fn test_search_without_search_url_returns_invariant_error() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let entity = build_entity(base_data(7, Some(SITE_URL), None), &fetcher, &policy);

        let err = entity
            .search("kurumi")
            .await
            .expect_err("missing search url should fail");
        assert!(err.to_string().contains("not found feed site url"));
    }

    #[tokio::test]
    async fn test_search_maps_format_error_to_external_error() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        // 花括号不配对，formatx 解析模板时会失败
        let entity = build_entity(
            base_data(
                7,
                Some(SITE_URL),
                Some("https://mikanani.me/RSS/Search?q={"),
            ),
            &fetcher,
            &policy,
        );

        let err = entity
            .search("kurumi")
            .await
            .expect_err("invalid template should fail");
        assert!(err.to_string().contains("format search url failed"));
        assert!(fetcher.fetched_urls().is_empty());
    }

    #[tokio::test]
    async fn test_set_updates_metadata() {
        let fetcher = MockFetcher::new(FetchOutcome::Data(empty_data()), vec!["new-key"]);
        let policy = Arc::new(MockAccessPolicy::new(true));
        let mut entity = build_entity(base_data(9, None, None), &fetcher, &policy);

        assert_eq!(entity.id(), 9);
        entity
            .set(
                "新标题".to_string(),
                Some(SITE_URL.to_string()),
                Some(SEARCH_URL.to_string()),
            )
            .await
            .expect("update metadata should succeed");

        assert_eq!(entity.title(), "新标题");
        assert_eq!(entity.site_url(), Some(SITE_URL));
        assert_eq!(entity.search_url(), Some(SEARCH_URL));
        assert_eq!(entity.get_base_data().metadata.title, "新标题");
        assert_eq!(entity.get_base_data().metadata.source_key, "new-key");
    }
}
