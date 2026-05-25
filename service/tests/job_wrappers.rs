use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anime::{
    animes::Animes,
    repository::{AnimeRepository, AnimeSnapshot},
    AnimeCaps,
};
use async_trait::async_trait;
use domain::anime::capability::{AnimeLockCap, AnimeMetadataUpdateCap};
use domain::rule::capability::RuleWriterCap;
use domain::shared::biz::{BizContext, BizFactory};
use domain::subscription::capability::{
    SubscriptionMatchCap, SubscriptionSearchCap, SubscriptionToggleCap,
};
use domain::{
    anime::{
        AirDate, AnimeId, AnimeListQuery, AnimeMetadata, AnimeMetadataRepository,
        AnimeStateRepository, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount,
        ReplaceAnimeMetadataResult, SeasonNumber,
    },
    feed::{
        FeedSource, FeedSourceId, Resource, ResourceId, ResourceRepository, ResourceSource,
        SpaceFeedRepository,
    },
    rule::{MatchingRule, MatchingRuleId, SpaceRuleRepository},
    shared::{error::DomainError, identifier::IdSequence},
    space::{PersonalSpaceBinding, Space, SpaceId, SpaceRepository},
    subscription::{
        LatestMatchRecord, MatchRecord, MatchRecordRepository, MatchResourceId, PoolSubLink,
        SearchPoolEntry, SearchPoolEntryData, SearchPoolRepository, SubscriptionAnime,
        SubscriptionAnimeRepository, SubscriptionSearchState,
    },
    user::UserId,
};
use feed::{
    contracts::{FeedData, FeedFetcher, FetchedFeedItem, ResolveFeedSource, ResolvedFeedSource},
    Feeds, Resources,
};
use service::{
    job::{CheckMissingEpisodesJob, FetchResourcesJob, Job, MatchResourcesJob},
    subscription::service::{SubscriptionService, SubscriptionServiceDependencies},
};
use space::Spaces;
use subscription::{
    action::{MatchedResource, RunMatchedResource},
    missing_episodes::MissingEpisodeChecker,
    SubscriptionAnimes, SubscriptionCaps,
};

fn sample_metadata() -> AnimeMetadata {
    AnimeMetadata {
        id: AnimeId(7),
        titles: AnimeTitleSet {
            original_ja: "Show".to_string(),
            localized_zh_cn: "节目".to_string(),
            localized_zh_tw: "節目".to_string(),
            search_name: "show".to_string(),
            aliases: vec![],
        },
        broadcast_weekday: BroadcastWeekday(1),
        planned_episode_count: PlannedEpisodeCount(12),
        air_date: AirDate("2026-04-01".to_string()),
        season: SeasonNumber(1),
    }
}

fn full_site_source() -> FeedSource {
    FeedSource {
        id: FeedSourceId("dmhy-site".to_string()),
        title: "DMHY".to_string(),
        site_url: Some("https://share.dmhy.org/topics/rss/rss.xml".to_string()),
        search_url: None,
        source_key: Some("dmhy-source".to_string()),
    }
}

fn matching_rule(id: &str, name: &str, active: bool) -> MatchingRule {
    MatchingRule {
        id: MatchingRuleId(id.to_string()),
        name: name.to_string(),
        order: 1,
        pattern: format!(r"^\[{name}\].*"),
        active,
    }
}

#[derive(Default)]
struct StubCatalog {
    items: Mutex<Vec<AnimeSnapshot>>,
}

#[async_trait]
impl AnimeRepository for StubCatalog {
    async fn list(&self, _filter: AnimeListQuery) -> Result<Vec<AnimeSnapshot>, DomainError> {
        Ok(self.items.lock().expect("items").clone())
    }

    async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeSnapshot>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("items")
            .iter()
            .find(|item| item.metadata.id == anime_id)
            .cloned())
    }

    async fn list_by_ids(&self, anime_ids: &[AnimeId]) -> Result<Vec<AnimeSnapshot>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("items")
            .iter()
            .filter(|item| anime_ids.contains(&item.metadata.id))
            .cloned()
            .collect())
    }
}

struct NoopMetadataRepository;

#[async_trait]
impl AnimeMetadataRepository for NoopMetadataRepository {
    async fn create_anime_metadata(&self, _metadata: &AnimeMetadata) -> Result<(), DomainError> {
        Ok(())
    }

    async fn replace_anime_metadata(
        &self,
        _entries: &[AnimeMetadata],
    ) -> Result<ReplaceAnimeMetadataResult, DomainError> {
        Ok(ReplaceAnimeMetadataResult {
            new_anime_ids: vec![],
        })
    }
}

struct NoopAnimeStateRepository;

struct NoopLocker;
#[async_trait]
impl AnimeLockCap for NoopLocker {
    async fn write_lock_status(
        &self,
        _anime_id: AnimeId,
        _locked: bool,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NoopMetadataUpdater;
#[async_trait]
impl AnimeMetadataUpdateCap for NoopMetadataUpdater {
    async fn update_metadata(
        &self,
        _anime_id: AnimeId,
        _metadata: &domain::anime::AnimeMetadata,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

#[async_trait]
impl AnimeStateRepository for NoopAnimeStateRepository {
    async fn set_metadata_locked(
        &self,
        _anime_id: AnimeId,
        _locked: bool,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NoopRuleWriter;
#[async_trait]
impl RuleWriterCap for NoopRuleWriter {
    async fn write_rule(
        &self,
        _scope: (&str, i64),
        _rule_id: &MatchingRuleId,
        _name: &str,
        _order: u32,
        _pattern: &str,
        _active: bool,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

struct NoopSearchPool;
#[async_trait]
impl SearchPoolRepository for NoopSearchPool {
    async fn insert_pool_entries(
        &self,
        _entries: &[SearchPoolEntryData],
    ) -> Result<Vec<i64>, DomainError> {
        Ok(vec![])
    }
    async fn insert_sub_links(&self, _links: &[PoolSubLink]) -> Result<(), DomainError> {
        Ok(())
    }
    async fn list_distinct_feed_ids(&self) -> Result<Vec<FeedSourceId>, DomainError> {
        Ok(vec![])
    }
    async fn pick_random(
        &self,
        _feed_id: &FeedSourceId,
    ) -> Result<Option<SearchPoolEntry>, DomainError> {
        Ok(None)
    }
    async fn delete_entry(&self, _id: i64) -> Result<(), DomainError> {
        Ok(())
    }
    async fn delete_sub_links_by_pool(&self, _pool_id: i64) -> Result<(), DomainError> {
        Ok(())
    }
    async fn cleanup_by_subscription(
        &self,
        _user_id: UserId,
        _space_id: SpaceId,
        _anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn count_by_anime(&self, _anime_id: AnimeId) -> Result<i64, DomainError> {
        Ok(0)
    }
    async fn count_distinct_anime(&self) -> Result<i64, DomainError> {
        Ok(0)
    }
    async fn count_pending_links(&self) -> Result<i64, DomainError> {
        Ok(0)
    }
}

#[derive(Default)]
struct StubClock;

impl user::gateway::EpochClock for StubClock {
    fn now_epoch_seconds(&self) -> i64 {
        1_900_000_000
    }
}

struct StubRegexProvider;

impl domain::rule::RegexProvider for StubRegexProvider {
    fn is_match(
        &self,
        pattern: &str,
        text: &str,
    ) -> Result<bool, domain::shared::error::DomainError> {
        let regex = regex::Regex::new(pattern)
            .map_err(|_| domain::shared::error::DomainError::InvariantViolation("invalid regex"))?;
        Ok(regex.is_match(text))
    }
}

#[derive(Default)]
struct InMemorySpaceFeeds {
    items: Mutex<HashMap<SpaceId, Vec<FeedSource>>>,
}

#[async_trait]
impl SpaceFeedRepository for InMemorySpaceFeeds {
    async fn find_space_feeds(&self, space_id: SpaceId) -> Result<Vec<FeedSource>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("space feeds")
            .get(&space_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn list_space_feeds(&self) -> Result<Vec<FeedSource>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("space feeds")
            .values()
            .flat_map(|sources| sources.clone())
            .collect())
    }

    async fn save_space_feed(
        &self,
        space_id: SpaceId,
        source: &FeedSource,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("space feeds");
        let sources = items.entry(space_id).or_default();
        if let Some(existing) = sources.iter_mut().find(|item| item.id == source.id) {
            *existing = source.clone();
        } else {
            sources.push(source.clone());
        }
        Ok(())
    }

    async fn delete_space_feed(
        &self,
        space_id: SpaceId,
        source_id: &FeedSourceId,
    ) -> Result<(), DomainError> {
        self.items
            .lock()
            .expect("space feeds")
            .entry(space_id)
            .or_default()
            .retain(|source| source.id != *source_id);
        Ok(())
    }

    async fn update_space_feed_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("space feeds");
        for sources in items.values_mut() {
            if let Some(source) = sources.iter_mut().find(|s| s.id == *source_id) {
                source.source_key = Some(source_key.to_string());
                break;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct InMemoryResources {
    resources: Mutex<HashMap<String, Resource>>,
    sources: Mutex<HashMap<String, Vec<ResourceSource>>>,
    last_since: Mutex<Option<i64>>,
}

#[async_trait]
impl ResourceRepository for InMemoryResources {
    async fn find_resource(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Option<Resource>, DomainError> {
        Ok(self
            .resources
            .lock()
            .expect("resources")
            .get(&resource_id.0)
            .cloned())
    }

    async fn list_resource_sources(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Vec<ResourceSource>, DomainError> {
        Ok(self
            .sources
            .lock()
            .expect("sources")
            .get(&resource_id.0)
            .cloned()
            .unwrap_or_default())
    }

    async fn save_resource(&self, resource: &Resource) -> Result<(), DomainError> {
        self.resources
            .lock()
            .expect("resources")
            .insert(resource.id.0.clone(), resource.clone());
        Ok(())
    }

    async fn save_resource_source(&self, source: &ResourceSource) -> Result<(), DomainError> {
        self.sources
            .lock()
            .expect("sources")
            .entry(source.resource_id.0.clone())
            .or_default()
            .push(source.clone());
        Ok(())
    }

    async fn latest_resources(&self, since: i64) -> Result<Vec<Resource>, DomainError> {
        *self.last_since.lock().expect("last_since") = Some(since);
        Ok(vec![])
    }

    async fn search_resources(&self, _keywords: &[String]) -> Result<Vec<Resource>, DomainError> {
        Ok(vec![])
    }
}

#[derive(Default)]
struct InMemorySubscriptions {
    items: Mutex<HashMap<(UserId, SpaceId, AnimeId), SubscriptionAnime>>,
}

#[async_trait]
impl SubscriptionAnimeRepository for InMemorySubscriptions {
    async fn find_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .get(&(user_id, space_id, anime_id))
            .cloned())
    }

    async fn list_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .filter(|subscription| subscription.space_id == space_id)
            .cloned()
            .collect())
    }

    async fn list_enabled_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .filter(|subscription| subscription.space_id == space_id && subscription.enabled)
            .cloned()
            .collect())
    }

    async fn list_all_enabled_subscriptions(&self) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .filter(|subscription| subscription.enabled)
            .cloned()
            .collect())
    }

    async fn pick_one_pending(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .find(|subscription| {
                subscription.search_state == SubscriptionSearchState::Pending
                    && subscription.enabled
            })
            .cloned())
    }

    async fn pick_one_localmatch(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .find(|subscription| subscription.search_state == SubscriptionSearchState::LocalMatch)
            .cloned())
    }

    async fn pick_one_pending_or_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        let items = self.items.lock().expect("subscriptions");
        // 优先 LocalMatch
        if let Some(sub) = items
            .values()
            .find(|s| s.search_state == SubscriptionSearchState::LocalMatch)
        {
            return Ok(Some(sub.clone()));
        }
        // 回退 Pending + enabled
        Ok(items
            .values()
            .find(|s| s.search_state == SubscriptionSearchState::Pending && s.enabled)
            .cloned())
    }

    async fn list_subscriptions_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .filter(|s| s.anime_id == anime_id)
            .cloned()
            .collect())
    }

    async fn has_enabled_subscription(&self, anime_id: AnimeId) -> Result<bool, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .any(|subscription| subscription.anime_id == anime_id && subscription.enabled))
    }

    async fn save_subscription(&self, subscription: &SubscriptionAnime) -> Result<(), DomainError> {
        self.items.lock().expect("subscriptions").insert(
            (
                subscription.user_id,
                subscription.space_id,
                subscription.anime_id,
            ),
            subscription.clone(),
        );
        Ok(())
    }

    async fn save_subscription_batch(
        &self,
        subscriptions: &[&SubscriptionAnime],
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("subscriptions");
        for subscription in subscriptions {
            items.insert(
                (
                    subscription.user_id,
                    subscription.space_id,
                    subscription.anime_id,
                ),
                (*subscription).clone(),
            );
        }
        Ok(())
    }

    async fn delete_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        self.items
            .lock()
            .expect("subscriptions")
            .remove(&(user_id, space_id, anime_id));
        Ok(())
    }

    async fn list_subscription_anime_ids_by_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<AnimeId>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("subscriptions")
            .values()
            .filter(|s| s.space_id == space_id)
            .map(|s| s.anime_id)
            .collect())
    }
}

#[async_trait]
impl SubscriptionToggleCap for InMemorySubscriptions {
    async fn write_enabled(
        &self,
        pk: (UserId, SpaceId, AnimeId),
        enabled: bool,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("subscriptions");
        if let Some(sub) = items.get_mut(&pk) {
            sub.enabled = enabled;
        }
        Ok(())
    }
}

#[async_trait]
impl SubscriptionMatchCap for InMemorySubscriptions {
    async fn write_match_result(
        &self,
        pk: (UserId, SpaceId, AnimeId),
        progress: i64,
        bound_rule: Option<String>,
        enabled: bool,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("subscriptions");
        if let Some(sub) = items.get_mut(&pk) {
            sub.progress = progress;
            sub.bound_rule_name = bound_rule;
            sub.enabled = enabled;
        }
        Ok(())
    }
}

#[async_trait]
impl SubscriptionSearchCap for InMemorySubscriptions {
    async fn write_search_state(
        &self,
        pk: (UserId, SpaceId, AnimeId),
        state: SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("subscriptions");
        if let Some(sub) = items.get_mut(&pk) {
            sub.search_state = state;
        }
        Ok(())
    }

    async fn batch_write_search_state(
        &self,
        pks: &[(UserId, SpaceId, AnimeId)],
        state: SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("subscriptions");
        pks.iter().for_each(|pk| {
            if let Some(sub) = items.get_mut(pk) {
                sub.search_state = state;
            }
        });
        Ok(())
    }
}

struct NoopMatchRecords;

#[async_trait]
impl MatchRecordRepository for NoopMatchRecords {
    async fn list_space_match_records(
        &self,
        _space_id: SpaceId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        Ok(vec![])
    }

    async fn list_match_records(
        &self,
        _user_id: UserId,
        _space_id: SpaceId,
        _anime_id: AnimeId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        Ok(vec![])
    }

    async fn list_latest_match_records(
        &self,
        _user_id: UserId,
        _space_id: SpaceId,
        _limit: usize,
    ) -> Result<Vec<LatestMatchRecord>, DomainError> {
        Ok(vec![])
    }

    async fn find_match_record(
        &self,
        _user_id: UserId,
        _space_id: SpaceId,
        _anime_id: AnimeId,
        _resource_id: &MatchResourceId,
    ) -> Result<Option<MatchRecord>, DomainError> {
        Ok(None)
    }

    async fn save_match_record(&self, _record: &MatchRecord) -> Result<(), DomainError> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingMatchRecords {
    saved: Mutex<Vec<MatchRecord>>,
}

#[async_trait]
impl MatchRecordRepository for RecordingMatchRecords {
    async fn list_space_match_records(
        &self,
        _space_id: SpaceId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        Ok(self.saved.lock().expect("records").clone())
    }

    async fn list_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        Ok(self
            .saved
            .lock()
            .expect("records")
            .iter()
            .filter(|record| {
                record.user_id == user_id
                    && record.space_id == space_id
                    && record.anime_id == anime_id
            })
            .cloned()
            .collect())
    }

    async fn list_latest_match_records(
        &self,
        _user_id: UserId,
        _space_id: SpaceId,
        _limit: usize,
    ) -> Result<Vec<LatestMatchRecord>, DomainError> {
        Ok(vec![])
    }

    async fn find_match_record(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        resource_id: &MatchResourceId,
    ) -> Result<Option<MatchRecord>, DomainError> {
        Ok(self
            .saved
            .lock()
            .expect("records")
            .iter()
            .find(|record| {
                record.user_id == user_id
                    && record.space_id == space_id
                    && record.anime_id == anime_id
                    && &record.resource_id == resource_id
            })
            .cloned())
    }

    async fn save_match_record(&self, record: &MatchRecord) -> Result<(), DomainError> {
        self.saved.lock().expect("records").push(record.clone());
        Ok(())
    }
}

#[derive(Default)]
struct InMemorySpaceRules {
    items: Mutex<HashMap<SpaceId, Vec<MatchingRule>>>,
}

#[async_trait]
impl SpaceRuleRepository for InMemorySpaceRules {
    async fn find_active_space_rules(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchingRule>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("space rules")
            .get(&space_id)
            .cloned()
            .into_iter()
            .flatten()
            .filter(|rule| rule.active)
            .collect())
    }

    async fn find_space_rule(
        &self,
        space_id: SpaceId,
        rule_id: &domain::rule::MatchingRuleId,
    ) -> Result<Option<MatchingRule>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("space rules")
            .get(&space_id)
            .and_then(|rules| rules.iter().find(|rule| &rule.id == rule_id).cloned()))
    }

    async fn find_space_rule_by_name(
        &self,
        space_id: SpaceId,
        name: &str,
    ) -> Result<Option<MatchingRule>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("space rules")
            .get(&space_id)
            .and_then(|rules| rules.iter().find(|rule| rule.name == name).cloned()))
    }

    async fn save_space_rule(
        &self,
        space_id: SpaceId,
        rule: &MatchingRule,
    ) -> Result<(), DomainError> {
        let mut items = self.items.lock().expect("space rules");
        let rules = items.entry(space_id).or_default();
        if let Some(index) = rules.iter().position(|existing| existing.id == rule.id) {
            rules[index] = rule.clone();
        } else {
            rules.push(rule.clone());
        }
        Ok(())
    }
}

fn noop_run_matched_resource() -> Arc<RunMatchedResource> {
    Arc::new(|_resource: MatchedResource| Box::pin(async { Ok::<(), _>(()) }))
}

fn resolve_feed_source() -> Arc<ResolveFeedSource> {
    Arc::new(|source| {
        Box::pin(async move {
            Ok(ResolvedFeedSource {
                source_key: source
                    .source_key
                    .unwrap_or_else(|| "dmhy-source".to_string()),
            })
        })
    })
}

#[derive(Default)]
struct StubFeedFetcher {
    fetch_data: Mutex<Vec<FeedData>>,
    search_data: Mutex<Vec<FeedData>>,
}

impl StubFeedFetcher {
    fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn with_fetch_data(data: Vec<FeedData>) -> Arc<Self> {
        Arc::new(Self {
            fetch_data: Mutex::new(data),
            search_data: Mutex::new(vec![]),
        })
    }
}

#[async_trait]
impl FeedFetcher for StubFeedFetcher {
    async fn fetch(&self, source: &FeedSource) -> Result<FeedData, DomainError> {
        let data = self.fetch_data.lock().expect("fetch data").first().cloned();
        Ok(data.unwrap_or_else(|| FeedData {
            source_key: source
                .source_key
                .clone()
                .unwrap_or_else(|| "dmhy-source".to_string()),
            items: vec![],
        }))
    }

    async fn search(&self, source: &FeedSource, _keyword: &str) -> Result<FeedData, DomainError> {
        let data = self
            .search_data
            .lock()
            .expect("search data")
            .first()
            .cloned();
        Ok(data.unwrap_or_else(|| FeedData {
            source_key: source
                .source_key
                .clone()
                .unwrap_or_else(|| "dmhy-source".to_string()),
            items: vec![],
        }))
    }
}

#[derive(Clone)]
struct NoopSpaces;

#[async_trait]
impl SpaceRepository for NoopSpaces {
    async fn save_subscription_space(&self, _space: &Space) -> Result<(), DomainError> {
        Ok(())
    }
    async fn find_subscription_space(
        &self,
        _space_id: SpaceId,
    ) -> Result<Option<Space>, DomainError> {
        Ok(None)
    }
    async fn find_personal_space_binding(
        &self,
        _user_id: UserId,
    ) -> Result<Option<PersonalSpaceBinding>, DomainError> {
        Ok(None)
    }
    async fn save_personal_space_binding(
        &self,
        _user_id: UserId,
        _binding: &PersonalSpaceBinding,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn list_auto_subscribing_spaces(&self) -> Result<Vec<Space>, DomainError> {
        Ok(Vec::new())
    }
    async fn find_personal_space_user_ids(
        &self,
        _space_ids: &[SpaceId],
    ) -> Result<Vec<(SpaceId, UserId)>, DomainError> {
        Ok(Vec::new())
    }
}

#[derive(Clone)]
struct NoopSpaceIdSequence;

#[async_trait]
impl IdSequence for NoopSpaceIdSequence {
    fn with_biz(
        &self,
        _: &domain::shared::biz::BizContext,
    ) -> Result<Arc<dyn IdSequence>, DomainError> {
        Ok(Arc::new(self.clone()))
    }
    async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError> {
        Ok(SpaceId(1))
    }
}

struct NoopBizFactory;
#[async_trait]
impl BizFactory for NoopBizFactory {
    async fn open_biz(&self) -> Result<BizContext, DomainError> {
        Err(DomainError::InvariantViolation("noop biz factory"))
    }
}

fn build_service(
    space_feeds: Arc<InMemorySpaceFeeds>,
    resources_repo: Arc<InMemoryResources>,
    subscriptions_repo: Arc<InMemorySubscriptions>,
    feed_fetcher: Arc<dyn FeedFetcher>,
) -> Arc<SubscriptionService> {
    build_service_with_rules(
        space_feeds,
        resources_repo,
        subscriptions_repo,
        Arc::new(InMemorySpaceRules::default()),
        feed_fetcher,
    )
}

fn build_service_with_rules(
    space_feeds: Arc<InMemorySpaceFeeds>,
    resources_repo: Arc<InMemoryResources>,
    subscriptions_repo: Arc<InMemorySubscriptions>,
    space_rules: Arc<InMemorySpaceRules>,
    feed_fetcher: Arc<dyn FeedFetcher>,
) -> Arc<SubscriptionService> {
    build_service_with_rules_action_and_records(
        space_feeds,
        resources_repo,
        subscriptions_repo,
        space_rules,
        feed_fetcher,
        noop_run_matched_resource(),
        Arc::new(NoopMatchRecords),
    )
}

fn build_service_with_rules_action_and_records(
    space_feeds: Arc<InMemorySpaceFeeds>,
    resources_repo: Arc<InMemoryResources>,
    subscriptions_repo: Arc<InMemorySubscriptions>,
    space_rules: Arc<InMemorySpaceRules>,
    feed_fetcher: Arc<dyn FeedFetcher>,
    run_matched_resource: Arc<RunMatchedResource>,
    match_records: Arc<dyn MatchRecordRepository>,
) -> Arc<SubscriptionService> {
    let anime_caps = AnimeCaps {
        locker: Arc::new(NoopLocker),
        metadata_updater: Arc::new(NoopMetadataUpdater),
    };
    let animes = Arc::new(Animes::new(
        anime_caps,
        Arc::new(NoopMetadataRepository),
        Arc::new(NoopAnimeStateRepository),
        Arc::new(StubCatalog {
            items: Mutex::new(vec![AnimeSnapshot {
                metadata: sample_metadata(),
                metadata_locked: false,
            }]),
        }),
    ));
    let feeds = Arc::new(Feeds::new(space_feeds, resolve_feed_source(), feed_fetcher));
    let rule_caps = rule::RuleCaps {
        writer: Arc::new(NoopRuleWriter),
    };
    let rules = Arc::new(rule::Rules::new(
        rule_caps,
        space_rules,
        Arc::new(StubRegexProvider),
    ));
    let resources = Arc::new(Resources::new(resources_repo, Arc::new(StubClock)));
    let sub_caps = SubscriptionCaps {
        toggle: subscriptions_repo.clone(),
        match_writer: subscriptions_repo.clone(),
        search: subscriptions_repo.clone(),
    };
    let subscriptions = Arc::new(SubscriptionAnimes::new(
        sub_caps,
        subscriptions_repo,
        match_records,
    ));
    let noop_spaces_repo = Arc::new(NoopSpaces);
    let noop_ids = Arc::new(NoopSpaceIdSequence);
    let spaces = Arc::new(Spaces::new(noop_spaces_repo, noop_ids));

    Arc::new(SubscriptionService::new(SubscriptionServiceDependencies {
        search_pool: Arc::new(NoopSearchPool),
        biz_factory: Arc::new(NoopBizFactory),
        subscriptions,
        spaces,
        animes,
        feeds,
        rules,
        resources,
        missing_episode_policy: Arc::new(MissingEpisodeChecker),
        run_matched_resource,
    }))
}

#[tokio::test]
async fn check_missing_episodes_job_executes_service() {
    let subscriptions = Arc::new(InMemorySubscriptions::default());
    subscriptions
        .save_subscription(&SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(7),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Stopped,
            progress: 0,
        })
        .await
        .expect("save subscription");
    let job = CheckMissingEpisodesJob::new(build_service(
        Arc::new(InMemorySpaceFeeds::default()),
        Arc::new(InMemoryResources::default()),
        subscriptions,
        StubFeedFetcher::empty(),
    ));

    job.run().await.expect("job succeeds");

    assert_eq!(job.name(), "check_missing_episodes");
}

#[tokio::test]
async fn fetch_resources_job_executes_service() {
    let space_feeds = Arc::new(InMemorySpaceFeeds::default());
    space_feeds
        .save_space_feed(SpaceId(1), &full_site_source())
        .await
        .expect("save feed");
    let resources_repo = Arc::new(InMemoryResources::default());
    let job = FetchResourcesJob::new(build_service(
        space_feeds,
        resources_repo.clone(),
        Arc::new(InMemorySubscriptions::default()),
        StubFeedFetcher::with_fetch_data(vec![FeedData {
            source_key: "dmhy-source".to_string(),
            items: vec![FetchedFeedItem {
                title: "Show".to_string(),
                source_url: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
                    .to_string(),
                torrent_content: None,
                published_at: Some(10),
            }],
        }]),
    ));

    job.run().await.expect("job succeeds");

    assert!(resources_repo
        .find_resource(&ResourceId(
            "0123456789abcdef0123456789abcdef01234567".to_string()
        ))
        .await
        .expect("find resource")
        .is_some());
    assert_eq!(job.name(), "fetch_resources");
}

#[tokio::test]
async fn match_resources_job_executes_service() {
    let resources_repo = Arc::new(InMemoryResources::default());
    let job = MatchResourcesJob::new(build_service(
        Arc::new(InMemorySpaceFeeds::default()),
        resources_repo.clone(),
        Arc::new(InMemorySubscriptions::default()),
        StubFeedFetcher::empty(),
    ));

    job.run().await.expect("job succeeds");

    let since = resources_repo
        .last_since
        .lock()
        .expect("last_since")
        .expect("since");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    assert!(
        since <= now && since >= now - 36000,
        "since={since} now={now}"
    );
    assert_eq!(job.name(), "match_resources");
}

#[tokio::test]
async fn bound_subscription_can_still_use_inactive_rule() {
    let space_feeds = Arc::new(InMemorySpaceFeeds::default());
    space_feeds
        .save_space_feed(SpaceId(1), &full_site_source())
        .await
        .expect("save feed");
    let subscriptions = Arc::new(InMemorySubscriptions::default());
    subscriptions
        .save_subscription(&SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(7),
            enabled: true,
            bound_rule_name: Some("ANi".to_string()),
            search_state: SubscriptionSearchState::Stopped,
            progress: 0,
        })
        .await
        .expect("save subscription");
    let space_rules = Arc::new(InMemorySpaceRules::default());
    space_rules
        .items
        .lock()
        .expect("space rules")
        .insert(SpaceId(1), vec![matching_rule("ani", "ANi", false)]);
    let service = build_service_with_rules(
        space_feeds,
        Arc::new(InMemoryResources::default()),
        subscriptions.clone(),
        space_rules,
        StubFeedFetcher::empty(),
    );

    let matched = service
        .match_resource(&Resource {
            id: ResourceId("resource-1".to_string()),
            title: "[ANi] Show - 01".to_string(),
            source_url: "magnet:?xt=urn:btih:resource1".to_string(),
            source_key: "dmhy-source".to_string(),
            published_at: None,
            created_at: 1_900_000_000,
        })
        .await
        .expect("match resource");

    assert!(matched);
    assert_eq!(
        subscriptions
            .find_subscription(UserId(1), SpaceId(1), AnimeId(7))
            .await
            .expect("find subscription")
            .expect("subscription")
            .progress,
        1
    );
}

#[tokio::test]
async fn matched_resource_is_persisted_only_after_download_action_succeeds() {
    let space_feeds = Arc::new(InMemorySpaceFeeds::default());
    space_feeds
        .save_space_feed(SpaceId(1), &full_site_source())
        .await
        .expect("save feed");
    let subscriptions = Arc::new(InMemorySubscriptions::default());
    subscriptions
        .save_subscription(&SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(7),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Stopped,
            progress: 0,
        })
        .await
        .expect("save subscription");
    let space_rules = Arc::new(InMemorySpaceRules::default());
    space_rules
        .items
        .lock()
        .expect("space rules")
        .insert(SpaceId(1), vec![matching_rule("ani", "ANi", true)]);
    let records = Arc::new(RecordingMatchRecords::default());
    let service = build_service_with_rules_action_and_records(
        space_feeds,
        Arc::new(InMemoryResources::default()),
        subscriptions.clone(),
        space_rules,
        StubFeedFetcher::empty(),
        noop_run_matched_resource(),
        records.clone(),
    );

    let matched = service
        .match_resource(&Resource {
            id: ResourceId("resource-1".to_string()),
            title: "[ANi] Show - 01".to_string(),
            source_url: "magnet:?xt=urn:btih:resource1".to_string(),
            source_key: "dmhy-source".to_string(),
            published_at: None,
            created_at: 1_900_000_000,
        })
        .await
        .expect("match resource");

    assert!(matched);
    assert_eq!(
        subscriptions
            .find_subscription(UserId(1), SpaceId(1), AnimeId(7))
            .await
            .expect("find subscription")
            .expect("subscription")
            .progress,
        1
    );
    assert_eq!(records.saved.lock().expect("records").len(), 1);
}

#[tokio::test]
async fn matched_resource_is_not_persisted_when_download_action_fails() {
    let space_feeds = Arc::new(InMemorySpaceFeeds::default());
    space_feeds
        .save_space_feed(SpaceId(1), &full_site_source())
        .await
        .expect("save feed");
    let subscriptions = Arc::new(InMemorySubscriptions::default());
    subscriptions
        .save_subscription(&SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(7),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Stopped,
            progress: 0,
        })
        .await
        .expect("save subscription");
    let space_rules = Arc::new(InMemorySpaceRules::default());
    space_rules
        .items
        .lock()
        .expect("space rules")
        .insert(SpaceId(1), vec![matching_rule("ani", "ANi", true)]);
    let records = Arc::new(RecordingMatchRecords::default());
    let failing_action: Arc<RunMatchedResource> = Arc::new(|_resource: MatchedResource| {
        Box::pin(async {
            Err::<(), _>(subscription::shared::error::ApplicationError::from(
                DomainError::InvariantViolation("download failed"),
            ))
        })
    });
    let service = build_service_with_rules_action_and_records(
        space_feeds,
        Arc::new(InMemoryResources::default()),
        subscriptions.clone(),
        space_rules,
        StubFeedFetcher::empty(),
        failing_action,
        records.clone(),
    );

    let matched = service
        .match_resource(&Resource {
            id: ResourceId("resource-1".to_string()),
            title: "[ANi] Show - 01".to_string(),
            source_url: "magnet:?xt=urn:btih:resource1".to_string(),
            source_key: "dmhy-source".to_string(),
            published_at: None,
            created_at: 1_900_000_000,
        })
        .await
        .expect("match resource");

    assert!(!matched);
    assert_eq!(
        subscriptions
            .find_subscription(UserId(1), SpaceId(1), AnimeId(7))
            .await
            .expect("find subscription")
            .expect("subscription")
            .progress,
        0
    );
    assert!(records.saved.lock().expect("records").is_empty());
}

#[tokio::test]
async fn inactive_rule_is_not_selected_for_unbound_subscription() {
    let space_feeds = Arc::new(InMemorySpaceFeeds::default());
    space_feeds
        .save_space_feed(SpaceId(1), &full_site_source())
        .await
        .expect("save feed");
    let subscriptions = Arc::new(InMemorySubscriptions::default());
    subscriptions
        .save_subscription(&SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(7),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Stopped,
            progress: 0,
        })
        .await
        .expect("save subscription");
    let space_rules = Arc::new(InMemorySpaceRules::default());
    space_rules
        .items
        .lock()
        .expect("space rules")
        .insert(SpaceId(1), vec![matching_rule("ani", "ANi", false)]);
    let service = build_service_with_rules(
        space_feeds,
        Arc::new(InMemoryResources::default()),
        subscriptions,
        space_rules,
        StubFeedFetcher::empty(),
    );

    let matched = service
        .match_resource(&Resource {
            id: ResourceId("resource-1".to_string()),
            title: "[ANi] Show - 01".to_string(),
            source_url: "magnet:?xt=urn:btih:resource1".to_string(),
            source_key: "dmhy-source".to_string(),
            published_at: None,
            created_at: 1_900_000_000,
        })
        .await
        .expect("match resource");

    assert!(!matched);
}
