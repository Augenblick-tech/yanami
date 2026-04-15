use std::sync::Arc;

use domain::{
    feed::{FeedSource, FeedSourceId, SpaceFeedRepository},
    shared::biz::BizContext,
    shared::error::DomainError,
    space::SpaceId,
};

use crate::{
    contracts::{FeedFetcher, FeedSourceKeyUpdater, ResolveFeedSource},
    entity::{validate_source_set, FeedEntity},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeedListQuery {
    pub space_id: Option<SpaceId>,
    pub with_site_url: bool,
    pub with_search_url: bool,
}

#[derive(Clone)]
pub struct Feeds {
    space_repository: Arc<dyn SpaceFeedRepository>,
    resolve_source: Arc<ResolveFeedSource>,
    feed_fetcher: Arc<dyn FeedFetcher>,
}

impl Feeds {
    pub fn new(
        space_repository: Arc<dyn SpaceFeedRepository>,
        resolve_source: Arc<ResolveFeedSource>,
        feed_fetcher: Arc<dyn FeedFetcher>,
    ) -> Self {
        Self {
            space_repository,
            resolve_source,
            feed_fetcher,
        }
    }

    pub fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            space_repository: self.space_repository.with_biz(biz)?,
            resolve_source: self.resolve_source.clone(),
            feed_fetcher: self.feed_fetcher.clone(),
        })
    }

    pub async fn list(&self, query: FeedListQuery) -> Result<Vec<FeedEntity>, DomainError> {
        let sources = match query.space_id {
            Some(space_id) => self.space_repository.find_space_feeds(space_id).await?,
            None => self.space_repository.list_space_feeds().await?,
        };
        self.build_list(sources, query.space_id.is_none(), &query)
    }

    pub async fn create(&self, source: FeedSource) -> Result<FeedEntity, DomainError> {
        FeedEntity::new_with_source_key_updater(
            self.resolve_source(source).await?,
            self.feed_fetcher.clone(),
            self.source_key_updater(),
        )
    }

    pub async fn save_source(
        &self,
        space_id: SpaceId,
        source: FeedSource,
    ) -> Result<FeedEntity, DomainError> {
        let incoming = self.create(source).await?.into_snapshot();
        let mut sources = self.space_repository.find_space_feeds(space_id).await?;
        let source_to_save = merge_single_source(&mut sources, incoming);
        validate_source_set(&sources)?;
        self.space_repository
            .save_space_feed(space_id, &source_to_save)
            .await?;
        FeedEntity::new_with_source_key_updater(
            source_to_save,
            self.feed_fetcher.clone(),
            self.source_key_updater(),
        )
    }

    pub async fn delete_source(
        &self,
        space_id: SpaceId,
        source_id: FeedSourceId,
    ) -> Result<(), DomainError> {
        if source_id.0.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "feed source id cannot be empty",
            ));
        }
        self.space_repository
            .delete_space_feed(space_id, &source_id)
            .await
    }

    fn build_list(
        &self,
        mut sources: Vec<FeedSource>,
        deduplicate_origins: bool,
        query: &FeedListQuery,
    ) -> Result<Vec<FeedEntity>, DomainError> {
        sources.sort_by(|left, right| left.id.0.cmp(&right.id.0));
        if query.with_site_url || query.with_search_url {
            sources.retain(|source| {
                (query.with_site_url && has_non_empty_site_url(source))
                    || (query.with_search_url && has_non_empty_search_url(source))
            });
        }
        if deduplicate_origins {
            deduplicate_by_source_key(&mut sources)?;
        }
        validate_source_set(&sources)?;
        sources
            .into_iter()
            .map(|source| {
                FeedEntity::new_with_source_key_updater(
                    source,
                    self.feed_fetcher.clone(),
                    self.source_key_updater(),
                )
            })
            .collect()
    }

    async fn resolve_source(&self, mut source: FeedSource) -> Result<FeedSource, DomainError> {
        let resolved = (self.resolve_source)(source.clone()).await?;
        if source.id.0.trim().is_empty() {
            source.id = FeedSourceId(feed_source_id_from_key(&resolved.source_key));
        }
        source.source_key = Some(resolved.source_key);
        Ok(source)
    }

    fn source_key_updater(&self) -> Arc<dyn FeedSourceKeyUpdater> {
        Arc::new(RepositoryFeedSourceKeyUpdater {
            repository: self.space_repository.clone(),
        })
    }
}

struct RepositoryFeedSourceKeyUpdater {
    repository: Arc<dyn SpaceFeedRepository>,
}

#[async_trait::async_trait]
impl FeedSourceKeyUpdater for RepositoryFeedSourceKeyUpdater {
    async fn update_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError> {
        self.repository
            .update_space_feed_source_key(source_id, source_key)
            .await
    }
}

fn merge_single_source(sources: &mut Vec<FeedSource>, incoming: FeedSource) -> FeedSource {
    if let Some(index) = sources
        .iter()
        .position(|source| source.id.0 == incoming.id.0)
    {
        sources[index] = incoming.clone();
        return incoming;
    }

    if let Some(incoming_key) = incoming.source_key.as_deref() {
        if let Some(index) = sources.iter().position(|source| {
            source
                .source_key
                .as_deref()
                .is_some_and(|source_key| source_key == incoming_key)
        }) {
            let mut merged = incoming;
            merged.id = sources[index].id.clone();
            sources[index] = merged.clone();
            return merged;
        }
    }

    sources.push(incoming.clone());
    incoming
}

fn feed_source_id_from_key(source_key: &str) -> String {
    let id = source_key
        .trim()
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || matches!(char, '-' | '_') {
                char.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if id.is_empty() {
        "feed".to_string()
    } else {
        format!("feed-{id}")
    }
}

fn has_non_empty_site_url(source: &FeedSource) -> bool {
    source
        .site_url
        .as_deref()
        .is_some_and(|u| !u.trim().is_empty())
}

fn has_non_empty_search_url(source: &FeedSource) -> bool {
    source
        .search_url
        .as_deref()
        .is_some_and(|u| !u.trim().is_empty())
}

fn deduplicate_by_source_key(sources: &mut Vec<FeedSource>) -> Result<(), DomainError> {
    let mut seen_source_keys = std::collections::HashSet::new();
    let mut deduplicated = Vec::new();

    for source in sources.drain(..) {
        let Some(source_key) = source.source_key.clone() else {
            deduplicated.push(source);
            continue;
        };
        if source_key.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "feed source key cannot be empty",
            ));
        }
        if seen_source_keys.insert(source_key) {
            deduplicated.push(source);
        }
    }

    *sources = deduplicated;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    use async_trait::async_trait;
    use domain::feed::{FeedSource, FeedSourceId};

    use crate::contracts::{FeedData, FeedFetcher, ResolvedFeedSource};

    use super::*;

    fn source(id: &str, source_key: &str) -> FeedSource {
        FeedSource {
            id: FeedSourceId(id.to_string()),
            title: id.to_string(),
            site_url: Some(format!("https://{id}.example/rss.xml")),
            search_url: Some(format!("https://{id}.example/rss.xml?keyword={{}}")),
            source_key: Some(source_key.to_string()),
        }
    }

    #[derive(Default)]
    struct RecordingSpaceFeedRepository {
        sources: Mutex<Vec<FeedSource>>,
        saved: Mutex<Vec<FeedSource>>,
        deleted: Mutex<Vec<FeedSourceId>>,
    }

    #[async_trait]
    impl SpaceFeedRepository for RecordingSpaceFeedRepository {
        async fn find_space_feeds(
            &self,
            _space_id: SpaceId,
        ) -> Result<Vec<FeedSource>, DomainError> {
            Ok(self.sources.lock().expect("sources").clone())
        }

        async fn list_space_feeds(&self) -> Result<Vec<FeedSource>, DomainError> {
            Ok(self.sources.lock().expect("sources").clone())
        }

        async fn save_space_feed(
            &self,
            _space_id: SpaceId,
            source: &FeedSource,
        ) -> Result<(), DomainError> {
            self.saved.lock().expect("saved").push(source.clone());
            let mut sources = self.sources.lock().expect("sources");
            if let Some(existing) = sources.iter_mut().find(|item| item.id == source.id) {
                *existing = source.clone();
            } else {
                sources.push(source.clone());
            }
            Ok(())
        }

        async fn delete_space_feed(
            &self,
            _space_id: SpaceId,
            source_id: &FeedSourceId,
        ) -> Result<(), DomainError> {
            self.deleted
                .lock()
                .expect("deleted")
                .push(source_id.clone());
            self.sources
                .lock()
                .expect("sources")
                .retain(|source| source.id != *source_id);
            Ok(())
        }

        async fn update_space_feed_source_key(
            &self,
            source_id: &FeedSourceId,
            source_key: &str,
        ) -> Result<(), DomainError> {
            let mut sources = self.sources.lock().expect("sources");
            if let Some(source) = sources.iter_mut().find(|s| s.id == *source_id) {
                source.source_key = Some(source_key.to_string());
            }
            Ok(())
        }
    }

    struct NoopFeedFetcher;

    #[async_trait]
    impl FeedFetcher for NoopFeedFetcher {
        async fn fetch(&self, source: &FeedSource) -> Result<FeedData, DomainError> {
            Ok(FeedData {
                source_key: source
                    .source_key
                    .clone()
                    .unwrap_or_else(|| source.id.0.clone()),
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
                    .unwrap_or_else(|| source.id.0.clone()),
                items: vec![],
            })
        }
    }

    fn feeds(
        repository: Arc<RecordingSpaceFeedRepository>,
        resolve_count: Arc<AtomicUsize>,
    ) -> Feeds {
        Feeds::new(
            repository,
            Arc::new(move |source: FeedSource| {
                resolve_count.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    Ok(ResolvedFeedSource {
                        source_key: source.title.to_lowercase(),
                    })
                })
            }),
            Arc::new(NoopFeedFetcher),
        )
    }

    #[test]
    fn deduplicate_by_source_key_keeps_one_source_per_parsed_key() {
        let mut sources = vec![
            source("dmhy-site", "dmhy"),
            source("dmhy-search", "dmhy"),
            source("mikan", "mikan"),
        ];

        deduplicate_by_source_key(&mut sources).expect("deduplicate");

        assert_eq!(
            sources
                .iter()
                .map(|source| source.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["dmhy-site", "mikan"]
        );
    }

    #[test]
    fn source_can_have_fetch_and_search_urls_together() {
        let source = source("dmhy", "dmhy");

        assert!(has_non_empty_site_url(&source));
        assert!(has_non_empty_search_url(&source));
        validate_source_set(&[source]).expect("combined source is valid");
    }

    #[tokio::test]
    async fn save_source_resolves_only_incoming_source() {
        let repository = Arc::new(RecordingSpaceFeedRepository::default());
        repository
            .sources
            .lock()
            .expect("sources")
            .push(source("existing", "existing"));
        let resolve_count = Arc::new(AtomicUsize::new(0));
        let feeds = feeds(repository.clone(), resolve_count.clone());

        let saved = feeds
            .save_source(
                SpaceId(1),
                FeedSource {
                    id: FeedSourceId(String::new()),
                    title: "DMHY".to_string(),
                    site_url: Some("https://dmhy.example/rss.xml".to_string()),
                    search_url: Some("https://dmhy.example/rss.xml?keyword={}".to_string()),
                    source_key: None,
                },
            )
            .await
            .expect("save source");

        assert_eq!(resolve_count.load(Ordering::SeqCst), 1);
        assert_eq!(saved.read_data().id.0, "feed-dmhy");
        assert_eq!(repository.saved.lock().expect("saved").len(), 1);
    }

    #[tokio::test]
    async fn delete_source_does_not_resolve_other_sources() {
        let repository = Arc::new(RecordingSpaceFeedRepository::default());
        repository
            .sources
            .lock()
            .expect("sources")
            .extend([source("dmhy", "dmhy"), source("mikan", "mikan")]);
        let resolve_count = Arc::new(AtomicUsize::new(0));
        let feeds = feeds(repository.clone(), resolve_count.clone());

        feeds
            .delete_source(SpaceId(1), FeedSourceId("dmhy".to_string()))
            .await
            .expect("delete source");

        assert_eq!(resolve_count.load(Ordering::SeqCst), 0);
        assert_eq!(
            repository.deleted.lock().expect("deleted").as_slice(),
            &[FeedSourceId("dmhy".to_string())]
        );
        assert_eq!(
            repository
                .sources
                .lock()
                .expect("sources")
                .iter()
                .map(|source| source.id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["mikan"]
        );
    }
}
