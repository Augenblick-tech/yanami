use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    anime::AnimeId, feed::FeedSourceId, shared::biz::BizContext, shared::error::DomainError,
    space::SpaceId, user::UserId,
};

pub mod capability;

/// 订阅剧集进度分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimeProgressState {
    NotStarted,
    InProgress,
    Completed,
}

/// 匹配时所需的 anime 上下文信息。
pub struct AnimeMatchingContext {
    pub planned_episode_count: i64,
    pub title_names: Vec<String>,
    pub air_date: String,
}

/// 缺集检查结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEpisodeAssessment {
    pub missing_count: i64,
    pub actual_count: i64,
    pub min_episode: i64,
    pub max_episode: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubscriptionSearchState {
    Stopped,
    Pending,
    Running,
    LocalMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionAnime {
    pub user_id: UserId,
    pub space_id: SpaceId,
    pub anime_id: AnimeId,
    pub enabled: bool,
    pub bound_rule_name: Option<String>,
    pub search_state: SubscriptionSearchState,
    pub progress: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchResourceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRecord {
    pub user_id: UserId,
    pub space_id: SpaceId,
    pub anime_id: AnimeId,
    pub resource_id: MatchResourceId,
    pub title: String,
    pub source_url: String,
    pub matched_rule_name: String,
    pub published_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestMatchRecord {
    pub anime_id: AnimeId,
    pub progress: i64,
    pub matched_rule_name: String,
    pub published_at: Option<i64>,
    pub created_at: i64,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    SubscriptionDisabled,
    TitleMismatch,
    NoMatchingRule,
    ReleaseDateInvalid,
    BoundRuleMismatch { bound: String, matched: String },
    AlreadyMatched,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::SubscriptionDisabled => write!(f, "subscription_disabled"),
            SkipReason::TitleMismatch => write!(f, "title_mismatch"),
            SkipReason::NoMatchingRule => write!(f, "no_matching_rule"),
            SkipReason::ReleaseDateInvalid => write!(f, "release_date_invalid"),
            SkipReason::BoundRuleMismatch { bound, matched } => {
                write!(f, "bound_rule_mismatch(bound={bound}, matched={matched})")
            }
            SkipReason::AlreadyMatched => write!(f, "already_matched"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchDecision {
    Ready {
        updated_subscription: Box<SubscriptionAnime>,
        record: Box<MatchRecord>,
    },
    Skip(SkipReason),
}

/// 资源发布后多长时间内参与匹配（秒）。
pub const MATCH_WINDOW_SECONDS: i64 = 3 * 3600;

#[async_trait]
pub trait SubscriptionAnimeRepository: Send + Sync {
    async fn find_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnime>, DomainError>;

    async fn list_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError>;

    async fn list_enabled_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError>;

    async fn list_all_enabled_subscriptions(&self) -> Result<Vec<SubscriptionAnime>, DomainError>;

    async fn pick_one_pending(&self) -> Result<Option<SubscriptionAnime>, DomainError>;

    async fn pick_one_localmatch(&self) -> Result<Option<SubscriptionAnime>, DomainError>;

    /// 优先 LocalMatch（任何 enabled），回退 Pending + enabled=1。
    /// 用于后台 local_match_runner 的单一入口查询。
    async fn pick_one_pending_or_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnime>, DomainError>;

    async fn list_subscriptions_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError>;

    async fn has_enabled_subscription(&self, anime_id: AnimeId) -> Result<bool, DomainError>;

    /// 轻量查询：只返回该空间下已订阅的番剧 ID，不拉全行和 match_record。
    async fn list_subscription_anime_ids_by_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<AnimeId>, DomainError>;

    async fn save_subscription(&self, subscription: &SubscriptionAnime) -> Result<(), DomainError>;

    async fn save_subscription_batch(&self, subscriptions: &[&SubscriptionAnime]) -> Result<(), DomainError>;

    async fn delete_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError>;

    fn with_biz(
        &self,
        _: &BizContext,
    ) -> Result<Arc<dyn SubscriptionAnimeRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "subscription anime repository does not support biz context",
        ))
    }
}

#[async_trait]
pub trait MatchRecordRepository: Send + Sync {
    async fn list_space_match_records(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchRecord>, DomainError>;

    async fn list_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Vec<MatchRecord>, DomainError>;

    async fn list_latest_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        limit: usize,
    ) -> Result<Vec<LatestMatchRecord>, DomainError>;

    async fn find_match_record(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        resource_id: &MatchResourceId,
    ) -> Result<Option<MatchRecord>, DomainError>;

    async fn save_match_record(&self, record: &MatchRecord) -> Result<(), DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn MatchRecordRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "match record repository does not support biz context",
        ))
    }
}

#[derive(Debug, Clone)]
pub struct SearchPoolEntry {
    pub id: i64,
    pub anime_id: AnimeId,
    pub feed_id: FeedSourceId,
    pub keyword: String,
    pub search_url: String,
}

#[derive(Debug, Clone)]
pub struct SearchPoolEntryData {
    pub anime_id: AnimeId,
    pub feed_id: FeedSourceId,
    pub keyword: String,
    pub search_url: String,
    pub created_at: i64,
}

pub struct PoolSubLink {
    pub pool_id: i64,
    pub user_id: UserId,
    pub space_id: SpaceId,
    pub anime_id: AnimeId,
}

#[async_trait]
pub trait SearchPoolRepository: Send + Sync {
    async fn insert_pool_entries(
        &self,
        entries: &[SearchPoolEntryData],
    ) -> Result<Vec<i64>, DomainError>;
    async fn insert_sub_links(&self, links: &[PoolSubLink]) -> Result<(), DomainError>;
    async fn list_distinct_feed_ids(&self) -> Result<Vec<FeedSourceId>, DomainError>;
    async fn pick_random(
        &self,
        feed_id: &FeedSourceId,
    ) -> Result<Option<SearchPoolEntry>, DomainError>;
    async fn delete_entry(&self, id: i64) -> Result<(), DomainError>;
    async fn delete_sub_links_by_pool(&self, pool_id: i64) -> Result<(), DomainError>;
    async fn cleanup_by_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError>;
    async fn count_by_anime(&self, anime_id: AnimeId) -> Result<i64, DomainError>;
    async fn count_distinct_anime(&self) -> Result<i64, DomainError>;
    async fn count_pending_links(&self) -> Result<i64, DomainError>;

    fn with_biz(
        &self,
        _: &BizContext,
    ) -> Result<Arc<dyn SearchPoolRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "search pool repository does not support biz context",
        ))
    }
}
