use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anime::animes::Animes as AnimeRoot;
use anime::contracts::AnimeUpdatedHandler;
use anime::source::{AnimeSource, SingleAnimeSource};
use domain::anime::{AnimeId, AnimeListQuery};
use domain::shared::error::DomainError;
use domain::space::SpaceId;
use domain::subscription::{AnimeProgressState, SubscriptionSearchState};
use domain::user::UserId;
use subscription::subscription_animes::SubscriptionAnimeListQuery;
use subscription::{SubscriptionAnimeEntity, SubscriptionAnimes};

use crate::shared::error::ApplicationError;
use crate::subscription::service::SubscriptionService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAnimeCalendarOutcome {
    pub fetched: usize,
    pub persisted: usize,
    pub new_anime_ids: Vec<AnimeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimeCollectionFilter {
    pub enabled: Option<bool>,
    pub search_enabled: Option<bool>,
    pub subscribed: Option<bool>,
    pub metadata_locked: Option<bool>,
    pub progress_state: Option<u8>,
    pub keyword: Option<String>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAnimeOutcome {
    pub items: Vec<AnimeItemView>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimeDashboardStats {
    pub total: usize,
    pub completed: usize,
    pub updating: usize,
    pub not_started: usize,
    pub paused: usize,
    pub subscribed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeDashboardQuarterStats {
    pub year: i32,
    pub month: u32,
    pub label: String,
    pub stats: AnimeDashboardStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeDashboardView {
    pub overall: AnimeDashboardStats,
    pub quarters: Vec<AnimeDashboardQuarterStats>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAnimeReleaseRecordsOutcome {
    pub anime_id: AnimeId,
    pub records: Vec<AnimeReleaseRecordView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeReleaseRecordView {
    pub resource_id: String,
    pub title: String,
    pub source_url: String,
    pub rule_name: String,
    pub published_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAnimeFlagOutcome {
    pub anime_id: AnimeId,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAnimeSearchEnabledOutcome {
    pub anime_id: AnimeId,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAnimeItemOutcome {
    pub item: AnimeItemView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeView {
    pub id: AnimeId,
    pub metadata: domain::anime::AnimeMetadata,
    pub search_state: SubscriptionSearchState,
    pub metadata_locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionAnimeView {
    pub subscribed: bool,
    pub enabled: bool,
    pub progress: u32,
    pub progress_state: AnimeProgressState,
    pub matched_rule_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeItemView {
    pub anime: AnimeView,
    pub subscription: SubscriptionAnimeView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestAnimeView {
    pub metadata: domain::anime::AnimeMetadata,
    pub episode: u32,
    pub rule_name: String,
    pub updated_at: i64,
}

pub struct AnimeService {
    animes: Arc<AnimeRoot>,
    subscriptions: Arc<SubscriptionAnimes>,
    subscription_service: Arc<SubscriptionService>,
    after_update_handlers: Vec<Arc<dyn AnimeUpdatedHandler>>,
}

impl AnimeService {
    pub fn new(
        animes: Arc<AnimeRoot>,
        subscriptions: Arc<SubscriptionAnimes>,
        subscription_service: Arc<SubscriptionService>,
        after_update_handlers: Vec<Arc<dyn AnimeUpdatedHandler>>,
    ) -> Self {
        Self {
            animes,
            subscriptions,
            subscription_service,
            after_update_handlers,
        }
    }

    pub async fn sync_source(
        &self,
        source: &dyn AnimeSource,
    ) -> Result<SyncAnimeCalendarOutcome, ApplicationError> {
        let entries = source.sync().await?;
        let fetched = entries.len();
        for entry in &entries {
            tracing::trace!(
                source = %source.name(),
                anime_id = entry.id.0,
                anime_name = %entry.titles.original_ja,
                localized_zh_cn = %entry.titles.localized_zh_cn,
                localized_zh_tw = %entry.titles.localized_zh_tw,
                air_date = %entry.air_date.0,
                planned_episode_count = entry.planned_episode_count.0,
                "sync_calendar: source metadata fetched"
            );
        }
        let entries: Vec<_> = entries
            .into_iter()
            .filter_map(
                |entry| match anime::entity::AnimeEntity::from_metadata(entry) {
                    Ok(entity) => Some(entity),
                    Err(error) => {
                        tracing::error!(
                            source = %source.name(),
                            ?error,
                            "sync_calendar: entity conversion failed, skipping"
                        );
                        None
                    }
                },
            )
            .collect();
        let new_entities = self.animes.save_list(&entries).await?;
        let new_anime_ids: Vec<AnimeId> = new_entities.iter().map(|a| a.id()).collect();
        for entry in &entries {
            for handler in &self.after_update_handlers {
                handler.on_anime_updated(entry.id()).await;
            }
        }
        Ok(SyncAnimeCalendarOutcome {
            fetched,
            persisted: new_entities.len(),
            new_anime_ids,
        })
    }

    pub async fn list_animes(
        &self,
        space_id: SpaceId,
        filter: AnimeCollectionFilter,
    ) -> Result<ListAnimeOutcome, ApplicationError> {
        let anime_filter = anime_filter(&filter);
        let anime_entities = self.animes.list(anime_filter).await?;
        let anime_ids = anime_entities
            .iter()
            .map(|anime| anime.id())
            .collect::<Vec<_>>();
        let subscriptions = self
            .subscriptions
            .list(SubscriptionAnimeListQuery {
                space_id,
                anime_ids: Some(anime_ids),
                enabled: None,
                search_state: None,
            })
            .await?;
        let subscriptions = subscriptions
            .into_iter()
            .map(|entity| (entity.read_data().anime_id, entity))
            .collect::<std::collections::HashMap<_, _>>();
        let mut matched: Vec<AnimeItemView> = anime_entities
            .into_iter()
            .filter_map(|anime| {
                let subscription = subscriptions.get(&anime.id());
                let item = AnimeItemView::from((&anime, subscription));
                anime_view_matches_filter(&item, &filter).then_some(item)
            })
            .collect();
        let total = matched.len();
        let page_start =
            ((filter.page.saturating_sub(1)) as usize).saturating_mul(filter.page_size as usize);
        let page_end = page_start
            .saturating_add(filter.page_size as usize)
            .min(total);
        let items = if page_start < total {
            matched.drain(page_start..page_end).collect()
        } else {
            Vec::new()
        };
        Ok(ListAnimeOutcome { items, total })
    }

    pub async fn list_latest(
        &self,
        space_id: SpaceId,
        limit: usize,
    ) -> Result<Vec<LatestAnimeView>, ApplicationError> {
        let subscriptions = self
            .subscriptions
            .list(SubscriptionAnimeListQuery {
                space_id,
                anime_ids: None,
                enabled: None,
                search_state: None,
            })
            .await?;
        let mut records = subscriptions
            .iter()
            .flat_map(|entity| {
                let progress = entity.read_data().progress;
                entity
                    .read_records()
                    .iter()
                    .cloned()
                    .map(move |record| (progress, record))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|record| std::cmp::Reverse(record.1.created_at));
        records.truncate(limit);
        let anime_ids = records
            .iter()
            .map(|item| item.1.anime_id)
            .collect::<Vec<_>>();
        let metadata_by_id = self
            .animes
            .load_many(&anime_ids)
            .await?
            .into_iter()
            .map(|anime| {
                let data = anime.into_snapshot();
                (data.metadata.id, data.metadata)
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut offsets = std::collections::HashMap::<AnimeId, u32>::new();

        records
            .into_iter()
            .map(|(progress, record)| {
                let metadata = metadata_by_id.get(&record.anime_id).cloned().ok_or(
                    domain::shared::error::DomainError::InvariantViolation(
                        "latest anime metadata is missing",
                    ),
                )?;
                let progress = u32::try_from(progress).map_err(|error| {
                    domain::shared::error::DomainError::external(
                        "latest anime progress decode failed",
                        error,
                    )
                })?;
                let offset = offsets.entry(record.anime_id).or_insert(0);
                let episode = progress.saturating_sub(*offset);
                *offset += 1;
                Ok(LatestAnimeView {
                    metadata,
                    episode,
                    rule_name: record.matched_rule_name,
                    updated_at: record.created_at,
                })
            })
            .collect()
    }

    pub async fn get_dashboard(
        &self,
        space_id: SpaceId,
    ) -> Result<AnimeDashboardView, ApplicationError> {
        let anime_entities = self.animes.list(AnimeListQuery::default()).await?;
        let subscriptions = self
            .subscriptions
            .list_all_in_space(space_id)
            .await?
            .into_iter()
            .map(|entity| (entity.read_data().anime_id, entity))
            .collect::<HashMap<_, _>>();
        let items = anime_entities
            .into_iter()
            .map(|anime| {
                let subscription = subscriptions.get(&anime.id());
                AnimeItemView::from((&anime, subscription))
            })
            .collect::<Vec<_>>();

        summarize_dashboard_items(items).map_err(Into::into)
    }

    pub async fn get_anime_item(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<AnimeItemView>, ApplicationError> {
        let anime = match self.animes.load(anime_id).await {
            Ok(anime) => anime,
            Err(domain::shared::error::DomainError::InvariantViolation("anime not found")) => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let subscription = self.subscriptions.load(user_id, space_id, anime_id).await?;
        Ok(Some(AnimeItemView::from((&anime, subscription.as_ref()))))
    }

    pub async fn list_release_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<ListAnimeReleaseRecordsOutcome, ApplicationError> {
        self.animes.load(anime_id).await?;
        let records = self
            .subscriptions
            .load(user_id, space_id, anime_id)
            .await?
            .map(|entity| {
                entity
                    .read_records()
                    .iter()
                    .map(|record| AnimeReleaseRecordView {
                        resource_id: record.resource_id.0.clone(),
                        title: record.title.clone(),
                        source_url: record.source_url.clone(),
                        rule_name: record.matched_rule_name.clone(),
                        published_at: record.published_at,
                        created_at: record.created_at,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(ListAnimeReleaseRecordsOutcome { anime_id, records })
    }

    pub async fn subscribe(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), ApplicationError> {
        self.animes.load(anime_id).await?;
        if self
            .subscriptions
            .load(user_id, space_id, anime_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        self.subscriptions
            .create(user_id, space_id, anime_id, true)
            .await?;
        self.subscription_service
            .start_anime_search(user_id, space_id, anime_id)
            .await?;
        Ok(())
    }

    pub async fn unsubscribe(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), ApplicationError> {
        self.animes.load(anime_id).await?;
        self.subscriptions
            .remove(user_id, space_id, anime_id)
            .await?;
        Ok(())
    }

    pub async fn set_active(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        enabled: bool,
    ) -> Result<UpdateAnimeFlagOutcome, ApplicationError> {
        self.animes.load(anime_id).await?;
        if enabled {
            let mut entity = self
                .subscriptions
                .load(user_id, space_id, anime_id)
                .await?
                .ok_or(DomainError::InvariantViolation("not subscribed"))?;
            entity.enable(&*self.subscriptions.caps.toggle).await?;
        } else {
            let biz = self.subscription_service.biz_factory.open_biz().await?;
            let subscriptions = self.subscriptions.with_biz(&biz).await?;
            let pool = self.subscription_service.search_pool.with_biz(&biz)?;
            let mut entity = subscriptions
                .load(user_id, space_id, anime_id)
                .await?
                .ok_or(DomainError::InvariantViolation("not subscribed"))?;
            entity
                .disable(
                    &*subscriptions.caps.toggle,
                    &*subscriptions.caps.search,
                    &*pool,
                )
                .await?;
            biz.commit().await?;
        }
        Ok(UpdateAnimeFlagOutcome { anime_id, enabled })
    }

    pub async fn set_search_enabled(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        enabled: bool,
    ) -> Result<UpdateAnimeSearchEnabledOutcome, ApplicationError> {
        self.animes.load(anime_id).await?;
        self.subscriptions
            .load(user_id, space_id, anime_id)
            .await?
            .ok_or(DomainError::InvariantViolation("not subscribed"))?;
        if enabled {
            self.subscription_service
                .start_anime_search(user_id, space_id, anime_id)
                .await?;
        } else {
            self.subscription_service
                .clean_anime_search_pool(user_id, space_id, anime_id)
                .await?;
        }
        Ok(UpdateAnimeSearchEnabledOutcome { anime_id, enabled })
    }

    pub async fn set_metadata_locked(
        &self,
        anime_id: AnimeId,
        enabled: bool,
    ) -> Result<UpdateAnimeFlagOutcome, ApplicationError> {
        let mut anime = self.animes.load(anime_id).await?;
        anime
            .set_metadata_locked(&*self.animes.caps.locker, enabled)
            .await?;
        let enabled = anime.read_data().metadata_locked;
        Ok(UpdateAnimeFlagOutcome { anime_id, enabled })
    }

    pub async fn patch_anime_item(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        search_enabled: Option<bool>,
        metadata_locked: Option<bool>,
    ) -> Result<UpdateAnimeItemOutcome, ApplicationError> {
        if let Some(search_enabled) = search_enabled {
            self.set_search_enabled(user_id, space_id, anime_id, search_enabled)
                .await?;
        }
        if let Some(metadata_locked) = metadata_locked {
            self.set_metadata_locked(anime_id, metadata_locked).await?;
        }
        Ok(UpdateAnimeItemOutcome {
            item: self
                .get_anime_item(user_id, space_id, anime_id)
                .await?
                .ok_or(domain::shared::error::DomainError::InvariantViolation(
                    "anime not found",
                ))?,
        })
    }

    pub async fn update_anime_metadata(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        new_metadata: domain::anime::AnimeMetadata,
    ) -> Result<UpdateAnimeItemOutcome, ApplicationError> {
        let mut anime = self.animes.load(anime_id).await?;
        anime
            .update_metadata(&*self.animes.caps.metadata_updater, new_metadata)
            .await?;
        let item = self
            .get_anime_item(user_id, space_id, anime_id)
            .await?
            .ok_or(domain::shared::error::DomainError::InvariantViolation(
                "anime not found",
            ))?;
        Ok(UpdateAnimeItemOutcome { item })
    }

    pub async fn preview_anime(
        &self,
        source: &dyn SingleAnimeSource,
        bgm_id: AnimeId,
    ) -> Result<domain::anime::AnimeMetadata, ApplicationError> {
        source.fetch_metadata(bgm_id).await.map_err(Into::into)
    }

    pub async fn create_anime(
        &self,
        metadata: domain::anime::AnimeMetadata,
    ) -> Result<AnimeId, ApplicationError> {
        let entity = self.animes.create(metadata).await?;
        let id = entity.id();
        for handler in &self.after_update_handlers {
            handler.on_anime_updated(id).await;
        }
        Ok(id)
    }
}

impl<'a>
    From<(
        &'a anime::entity::AnimeEntity,
        Option<&'a SubscriptionAnimeEntity>,
    )> for AnimeItemView
{
    fn from(
        (anime, subscription): (
            &'a anime::entity::AnimeEntity,
            Option<&'a SubscriptionAnimeEntity>,
        ),
    ) -> Self {
        let anime_data = anime.read_data();
        let progress = subscription
            .and_then(|item| u32::try_from(item.read_data().progress).ok())
            .unwrap_or(0);
        let matched_rule_name = subscription.and_then(|item| {
            item.read_records()
                .last()
                .map(|record| record.matched_rule_name.clone())
        });
        AnimeItemView {
            anime: AnimeView {
                id: anime.id(),
                metadata: anime_data.metadata.clone(),
                search_state: subscription
                    .as_ref()
                    .map(|item| item.read_data().search_state)
                    .unwrap_or(SubscriptionSearchState::Stopped),
                metadata_locked: anime_data.metadata_locked,
            },
            subscription: SubscriptionAnimeView {
                subscribed: subscription.is_some(),
                enabled: subscription
                    .as_ref()
                    .map(|item| item.read_data().enabled)
                    .unwrap_or(false),
                progress,
                progress_state: subscription
                    .as_ref()
                    .map(|item| item.progress_state(anime_data.metadata.planned_episode_count.0))
                    .unwrap_or(AnimeProgressState::NotStarted),
                matched_rule_name,
            },
        }
    }
}

fn anime_filter(filter: &AnimeCollectionFilter) -> AnimeListQuery {
    AnimeListQuery {
        metadata_locked: filter.metadata_locked,
        keyword: filter.keyword.clone(),
        year: filter.year,
        month: filter.month,
    }
}

fn anime_view_matches_filter(item: &AnimeItemView, filter: &AnimeCollectionFilter) -> bool {
    if filter
        .enabled
        .is_some_and(|enabled| item.subscription.enabled != enabled)
    {
        return false;
    }
    if filter.search_enabled.is_some_and(|enabled| {
        (item.anime.search_state != SubscriptionSearchState::Stopped) != enabled
    }) {
        return false;
    }
    if filter
        .subscribed
        .is_some_and(|subscribed| item.subscription.subscribed != subscribed)
    {
        return false;
    }
    if filter
        .metadata_locked
        .is_some_and(|locked| item.anime.metadata_locked != locked)
    {
        return false;
    }
    if let Some(progress_state) = filter.progress_state {
        if !item.subscription.subscribed {
            return false;
        }
        let state = item.subscription.progress_state;
        let matches = matches!(
            (progress_state, state),
            (0, AnimeProgressState::NotStarted)
                | (1, AnimeProgressState::InProgress)
                | (2, AnimeProgressState::Completed)
        );
        if !matches {
            return false;
        }
    }
    true
}

fn summarize_dashboard_items(
    items: Vec<AnimeItemView>,
) -> Result<AnimeDashboardView, domain::shared::error::DomainError> {
    let mut overall = AnimeDashboardStats::default();
    let mut quarters = BTreeMap::<(i32, u32), AnimeDashboardStats>::new();

    for item in items {
        apply_dashboard_item(&mut overall, &item);
        let quarter = item.anime.metadata.quarter()?;
        apply_dashboard_item(quarters.entry(quarter).or_default(), &item);
    }

    let quarters = quarters
        .into_iter()
        .rev()
        .map(|((year, month), stats)| AnimeDashboardQuarterStats {
            year,
            month,
            label: format!("{year}-Q{}", ((month - 1) / 3) + 1),
            stats,
        })
        .collect();

    Ok(AnimeDashboardView { overall, quarters })
}

fn apply_dashboard_item(stats: &mut AnimeDashboardStats, item: &AnimeItemView) {
    stats.total += 1;
    if item.subscription.subscribed {
        stats.subscribed += 1;
        match item.subscription.progress_state {
            AnimeProgressState::Completed => stats.completed += 1,
            AnimeProgressState::NotStarted => stats.not_started += 1,
            AnimeProgressState::InProgress if !item.subscription.enabled => stats.paused += 1,
            AnimeProgressState::InProgress => stats.updating += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::anime::{
        AirDate, AnimeMetadata, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount, SeasonNumber,
    };

    fn item(
        anime_id: i64,
        air_date: &str,
        subscribed: bool,
        enabled: bool,
        progress: u32,
        planned: i64,
    ) -> AnimeItemView {
        let state = if progress >= planned as u32 {
            AnimeProgressState::Completed
        } else if progress == 0 {
            AnimeProgressState::NotStarted
        } else {
            AnimeProgressState::InProgress
        };
        AnimeItemView {
            anime: AnimeView {
                id: AnimeId(anime_id),
                metadata: AnimeMetadata {
                    id: AnimeId(anime_id),
                    titles: AnimeTitleSet {
                        original_ja: format!("anime-{anime_id}"),
                        localized_zh_cn: String::new(),
                        localized_zh_tw: String::new(),
                        search_name: format!("anime-{anime_id}"),
                        aliases: vec![],
                    },
                    broadcast_weekday: BroadcastWeekday(1),
                    planned_episode_count: PlannedEpisodeCount(planned),
                    air_date: AirDate(air_date.to_string()),
                    season: SeasonNumber(1),
                },
                search_state: SubscriptionSearchState::Stopped,
                metadata_locked: false,
            },
            subscription: SubscriptionAnimeView {
                subscribed,
                enabled,
                progress,
                progress_state: state,
                matched_rule_name: None,
            },
        }
    }

    #[test]
    fn dashboard_stats_count_overall_and_quarterly_statuses() {
        let dashboard = summarize_dashboard_items(vec![
            item(1, "2026-04-01", true, true, 3, 12),
            item(2, "2026-04-02", true, false, 12, 12),
            item(3, "2026-04-03", false, false, 0, 12),
            item(4, "2026-07-01", true, false, 4, 12),
            item(5, "2026-07-02", true, true, 0, 12),
        ])
        .expect("dashboard");

        assert_eq!(
            dashboard.overall,
            AnimeDashboardStats {
                total: 5,
                completed: 1,
                updating: 1,
                not_started: 1,
                paused: 1,
                subscribed: 4,
            }
        );
        assert_eq!(dashboard.quarters.len(), 2);
        assert_eq!(dashboard.quarters[0].label, "2026-Q3");
        assert_eq!(
            dashboard.quarters[0].stats,
            AnimeDashboardStats {
                total: 2,
                completed: 0,
                updating: 0,
                not_started: 1,
                paused: 1,
                subscribed: 2,
            }
        );
        assert_eq!(dashboard.quarters[1].label, "2026-Q2");
        assert_eq!(
            dashboard.quarters[1].stats,
            AnimeDashboardStats {
                total: 3,
                completed: 1,
                updating: 1,
                not_started: 0,
                paused: 0,
                subscribed: 2,
            }
        );
    }

    fn metadata_with_air_date(air_date: &str) -> AnimeMetadata {
        AnimeMetadata {
            id: AnimeId(1),
            titles: AnimeTitleSet {
                original_ja: String::new(),
                localized_zh_cn: String::new(),
                localized_zh_tw: String::new(),
                search_name: String::new(),
                aliases: vec![],
            },
            broadcast_weekday: BroadcastWeekday(1),
            planned_episode_count: PlannedEpisodeCount(12),
            air_date: AirDate(air_date.to_string()),
            season: SeasonNumber(1),
        }
    }

    #[test]
    fn dashboard_quarter_uses_anime_season_boundaries() {
        assert_eq!(
            metadata_with_air_date("2026-02-28").quarter().unwrap(),
            (2026, 1)
        );
        assert_eq!(
            metadata_with_air_date("2026-03-01").quarter().unwrap(),
            (2026, 4)
        );
        assert_eq!(
            metadata_with_air_date("2026-12-01").quarter().unwrap(),
            (2027, 1)
        );
    }
}
