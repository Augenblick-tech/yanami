use std::collections::HashSet;
use std::sync::Arc;

use anime::animes::Animes;
use subscription::missing_episodes::MissingEpisodeChecker;
use domain::{
    anime::AnimeId,
    feed::{FeedSourceId, Resource, ResourceId},
    shared::biz::BizFactory,
    shared::error::DomainError,
    space::SpaceId,
    subscription::{
        capability::SubscriptionPk, AnimeMatchingContext, MatchDecision, PoolSubLink,
        SearchPoolEntryData, SearchPoolRepository, SubscriptionAnime, SubscriptionSearchState,
        MATCH_WINDOW_SECONDS,
    },
    user::UserId,
};
use feed::{FeedEntity, FeedListQuery, Feeds, ResourceListQuery, Resources};
use space::Spaces;
use subscription::{
    action::{MatchedResource, RunMatchedResource},
    entity::SubscriptionAnimeEntity,
    keywords::subscription_keywords,
    save_path::build_relative_save_path,
    SubscriptionAnimes,
};

use crate::shared::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResourcesOutcome {
    pub saved_count: usize,
    pub new_resource_ids: Vec<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResourcesOutcome {
    pub resource_count: usize,
    pub matched_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckMissingEpisodesOutcome {
    pub checked_subscription_count: usize,
    pub resumed_anime_count: usize,
}

pub struct SubscriptionService {
    pub search_pool: Arc<dyn SearchPoolRepository>,
    biz_factory: Arc<dyn BizFactory>,
    subscriptions: Arc<SubscriptionAnimes>,
    spaces: Arc<Spaces>,
    animes: Arc<Animes>,
    feeds: Arc<Feeds>,
    rules: Arc<rule::Rules>,
    resources: Arc<Resources>,
    missing_episode_policy: Arc<MissingEpisodeChecker>,
    run_matched_resource: Arc<RunMatchedResource>,
}

pub struct SubscriptionServiceDependencies {
    pub search_pool: Arc<dyn SearchPoolRepository>,
    pub biz_factory: Arc<dyn BizFactory>,
    pub subscriptions: Arc<SubscriptionAnimes>,
    pub spaces: Arc<Spaces>,
    pub animes: Arc<Animes>,
    pub feeds: Arc<Feeds>,
    pub rules: Arc<rule::Rules>,
    pub resources: Arc<Resources>,
    pub missing_episode_policy: Arc<MissingEpisodeChecker>,
    pub run_matched_resource: Arc<RunMatchedResource>,
}

impl SubscriptionService {
    pub fn new(dependencies: SubscriptionServiceDependencies) -> Self {
        Self {
            search_pool: dependencies.search_pool,
            biz_factory: dependencies.biz_factory,
            subscriptions: dependencies.subscriptions,
            spaces: dependencies.spaces,
            animes: dependencies.animes,
            feeds: dependencies.feeds,
            rules: dependencies.rules,
            resources: dependencies.resources,
            missing_episode_policy: dependencies.missing_episode_policy,
            run_matched_resource: dependencies.run_matched_resource,
        }
    }

    pub async fn match_resource(&self, resource: &Resource) -> Result<bool, ApplicationError> {
        let matched = self.apply_resource_to_all_subscriptions(resource).await?;
        Ok(matched)
    }

    pub async fn fetch_resources(&self) -> Result<FetchResourcesOutcome, ApplicationError> {
        let mut saved_count = 0usize;
        let mut new_resource_ids = Vec::new();

        let feeds = self
            .feeds
            .list(FeedListQuery {
                space_id: None,
                with_site_url: true,
                with_search_url: false,
            })
            .await?;
        for mut feed in feeds {
            let source_title = feed.read_data().title.clone();
            let feed_data = match feed.fetch().await {
                Ok(feed_data) => feed_data,
                Err(error) => {
                    tracing::error!(
                        source = %source_title,
                        %error,
                        "fetch feed source failed, skipping to next source"
                    );
                    continue;
                }
            };

            let fetched = self.resources.ingest(feed_data).await?;
            saved_count += fetched.len();
            new_resource_ids.extend(
                fetched
                    .into_iter()
                    .map(|resource| resource.read_data().id.clone()),
            );
        }

        Ok(FetchResourcesOutcome {
            saved_count,
            new_resource_ids,
        })
    }

    pub async fn match_resources(&self) -> Result<MatchResourcesOutcome, ApplicationError> {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            - MATCH_WINDOW_SECONDS;
        let resources = self
            .resources
            .list(ResourceListQuery {
                since: Some(since),
                keywords: None,
            })
            .await?;
        let mut matched_count = 0usize;

        for resource in &resources {
            match self.match_resource(resource.read_data()).await {
                Ok(true) => matched_count += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(
                        resource_id = %resource.read_data().id.0,
                        resource_title = %resource.read_data().title,
                        ?error,
                        "match_resource failed for single resource, skipping"
                    );
                }
            }
        }

        Ok(MatchResourcesOutcome {
            resource_count: resources.len(),
            matched_count,
        })
    }

    pub async fn found_pool_resources(
        &self,
        feed_data: feed::contracts::FeedData,
        anime_id: AnimeId,
    ) -> Result<(usize, usize), ApplicationError> {
        let remote = self.resources.ingest(feed_data).await?;
        let saved_count = remote.len();
        let mut matched_count = 0usize;

        let subscriptions = self.subscriptions.list_by_anime(anime_id).await?;
        for resource in &remote {
            for subscription in &subscriptions {
                match self
                    .apply_resource_to_subscription(
                        subscription.read_data().clone(),
                        resource.read_data(),
                    )
                    .await
                {
                    Ok(true) => matched_count += 1,
                    Ok(false) => {
                        tracing::debug!(
                            resource_id = %resource.read_data().id.0,
                            resource_title = %resource.read_data().title,
                            anime_id = %anime_id.0,
                            "found_pool_resources: resource not matched to any subscription"
                        );
                    }
                    Err(error) => {
                        tracing::error!(
                            resource_id = %resource.read_data().id.0,
                            ?error,
                            "found_pool_resources: match failed, skipping"
                        );
                    }
                }
            }
        }

        Ok((saved_count, matched_count))
    }

    /// 为该 anime 创建搜索池条目并关联订阅，设状态为 Pending。
    /// 所有 DB 操作在同一个 biz 事务中。
    pub async fn ensure_anime_search_pool(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), ApplicationError> {
        let metadata = self.animes.load(anime_id).await?.into_snapshot().metadata;
        let keywords = subscription_keywords(&metadata);
        let sources = self.search_sources_for_space(space_id).await?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut all_entries: Vec<(FeedSourceId, Vec<SearchPoolEntryData>)> = Vec::new();
        for source in &sources {
            let source_data = source.read_data();
            let Some(search_template) = source_data.search_url.as_deref() else { continue };
            let feed_id = source_data.id.clone();

            let entries: Vec<SearchPoolEntryData> = keywords
                .iter()
                .map(|keyword| SearchPoolEntryData {
                    anime_id,
                    feed_id: feed_id.clone(),
                    keyword: keyword.clone(),
                    search_url: render_search_url(search_template, keyword),
                    created_at: now,
                })
                .collect();

            all_entries.push((feed_id, entries));
        }

        let biz = self.biz_factory.open_biz().await?;
        let pool_repo = self.search_pool.with_biz(&biz)?;
        let subscriptions = self.subscriptions.with_biz(&biz).await?;

        let mut pool_ids = Vec::new();
        for (_feed_id, entries) in &all_entries {
            let ids = pool_repo.insert_pool_entries(entries).await?;
            pool_ids.extend(ids);
        }

        let links: Vec<PoolSubLink> = pool_ids
            .iter()
            .map(|&pool_id| PoolSubLink {
                pool_id,
                user_id,
                space_id,
                anime_id,
            })
            .collect();
        pool_repo.insert_sub_links(&links).await?;

        let mut entity = subscriptions
            .load(user_id, space_id, anime_id)
            .await?
            .ok_or(DomainError::InvariantViolation("subscription not found"))?;
        entity.start_search(&*subscriptions.caps.search).await?;
        subscriptions.save(&entity).await?;

        biz.commit().await?;
        Ok(())
    }

    /// 清理该订阅的池条目和孤儿条目，设状态为 Stopped。
    /// 所有 DB 操作在同一个 biz 事务中。
    pub async fn clean_anime_search_pool(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), ApplicationError> {
        let biz = self.biz_factory.open_biz().await?;
        let pool_repo = self.search_pool.with_biz(&biz)?;
        let subscriptions = self.subscriptions.with_biz(&biz).await?;

        pool_repo
            .cleanup_by_subscription(user_id, space_id, anime_id)
            .await?;
        let mut entity = subscriptions
            .load(user_id, space_id, anime_id)
            .await?
            .ok_or(DomainError::InvariantViolation("subscription not found"))?;
        entity.stop_search(&*subscriptions.caps.search).await?;
        subscriptions.save(&entity).await?;

        biz.commit().await?;
        Ok(())
    }

    pub async fn check_missing_episodes(
        &self,
    ) -> Result<CheckMissingEpisodesOutcome, ApplicationError> {
        let mut checked_subscription_count = 0usize;
        let mut resumed_anime_ids = HashSet::new();

        let subscriptions = self.subscriptions.list_enabled().await?;
        for subscription in subscriptions {
            checked_subscription_count += 1;
            let subscription_data = subscription.read_data();
            let loaded = match self
                .subscriptions
                .load(
                    subscription_data.user_id,
                    subscription_data.space_id,
                    subscription_data.anime_id,
                )
                .await
            {
                Ok(entity) => entity,
                Err(error) => {
                    tracing::error!(
                        anime_id = %subscription_data.anime_id.0,
                        ?error,
                        "check_missing_episodes: load subscription failed, skipping"
                    );
                    continue;
                }
            };
            let Some(mut entity) = loaded else {
                tracing::debug!(
                    anime_id = %subscription_data.anime_id.0,
                    "check_missing_episodes: subscription not found, skipping"
                );
                continue;
            };
            let titles = entity
                .read_records()
                .iter()
                .map(|record| record.title.clone())
                .collect::<Vec<_>>();
            let assessed = match self.missing_episode_policy.assess_missing_episodes(&titles) {
                Ok(assessment) => assessment,
                Err(error) => {
                    tracing::error!(
                        anime_id = %subscription_data.anime_id.0,
                        ?error,
                        "check_missing_episodes: assess failed, skipping"
                    );
                    continue;
                }
            };
            let Some(assessment) = assessed else {
            tracing::debug!(
                anime_id = %subscription_data.anime_id.0,
                "check_missing_episodes: no missing episodes, skipping"
            );
                continue;
            };
            let anime_id = subscription_data.anime_id;
            if let Err(error) = entity
                .resume_search(&*self.subscriptions.caps.search, assessment.missing_count)
                .await
            {
                tracing::error!(
                    anime_id = %anime_id.0,
                    ?error,
                    "check_missing_episodes: resume_search failed"
                );
                continue;
            }
            resumed_anime_ids.insert(anime_id);
        }

        Ok(CheckMissingEpisodesOutcome {
            checked_subscription_count,
            resumed_anime_count: resumed_anime_ids.len(),
        })
    }

    pub async fn match_local_resources(
        &self,
        user_id: domain::user::UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<usize, ApplicationError> {
        let metadata = self.animes.load(anime_id).await?.into_snapshot().metadata;
        let keywords = subscription_keywords(&metadata);
        let resources = self
            .resources
            .list(ResourceListQuery {
                since: None,
                keywords: Some(keywords),
            })
            .await?;
        let mut matched_count = 0usize;

        for resource in resources {
            let loaded = match self.subscriptions.load(user_id, space_id, anime_id).await {
                Ok(entity) => entity,
                Err(error) => {
                    tracing::error!(
                        anime_id = %anime_id.0,
                        resource_id = %resource.read_data().id.0,
                        ?error,
                        "match_local_resources: load subscription failed, skipping resource"
                    );
                    continue;
                }
            };
            let Some(entity) = loaded else {
                return Ok(matched_count);
            };
            match self
                .apply_resource_to_subscription(entity.into_snapshot(), resource.read_data())
                .await
            {
                Ok(true) => matched_count += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(
                        anime_id = %anime_id.0,
                        resource_id = %resource.read_data().id.0,
                        resource_title = %resource.read_data().title,
                        ?error,
                        "match_local_resources: apply failed, skipping resource"
                    );
                }
            }
        }

        Ok(matched_count)
    }

    pub async fn match_unbound_resources(&self) -> Result<usize, ApplicationError> {
        let subscriptions = self.subscriptions.list_enabled().await?;
        let mut matched_count = 0usize;

        for subscription in subscriptions {
            let subscription_data = subscription.read_data();
            if subscription_data.bound_rule_name.is_some() {
                continue;
            }
            match self
                .match_local_resources(
                    subscription_data.user_id,
                    subscription_data.space_id,
                    subscription_data.anime_id,
                )
                .await
            {
                Ok(count) => matched_count += count,
                Err(error) => {
                    tracing::error!(
                        anime_id = %subscription_data.anime_id.0,
                        ?error,
                        "match_unbound_resources: match_local_resources failed, skipping subscription"
                    );
                }
            }
        }

        Ok(matched_count)
    }

    async fn apply_resource_to_all_subscriptions(
        &self,
        resource: &Resource,
    ) -> Result<bool, ApplicationError> {
        let subscriptions = self.subscriptions.list_enabled().await?;
        for subscription in subscriptions {
            let subscription = subscription.into_snapshot();
            match self
                .apply_resource_to_subscription(subscription, resource)
                .await
            {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(
                        resource_id = %resource.id.0,
                        resource_title = %resource.title,
                        ?error,
                        "apply_resource_to_space: apply_resource_to_subscription failed, skipping subscription"
                    );
                }
            }
        }
        Ok(false)
    }

    async fn apply_resource_to_subscription(
        &self,
        subscription: SubscriptionAnime,
        resource: &Resource,
    ) -> Result<bool, ApplicationError> {
        if !self
            .resource_visible_to_subscription(subscription.space_id, resource)
            .await?
        {
            tracing::debug!(
                resource_id = %resource.id.0,
                resource_title = %resource.title,
                anime_id = %subscription.anime_id.0,
                "apply_resource_to_subscription: resource not visible to subscription, skipping"
            );
            return Ok(false);
        }

        let Some(mut entity) = self
            .subscriptions
            .load(
                subscription.user_id,
                subscription.space_id,
                subscription.anime_id,
            )
            .await?
        else {
            tracing::debug!(
                resource_id = %resource.id.0,
                resource_title = %resource.title,
                anime_id = %subscription.anime_id.0,
                "apply_resource_to_subscription: subscription entity not found, skipping"
            );
            return Ok(false);
        };
        let metadata = self
            .animes
            .load(subscription.anime_id)
            .await?
            .into_snapshot()
            .metadata;
        let keywords = subscription_keywords(&metadata);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let matched_rule = self
            .match_subscription_rule(&subscription, &resource.title)
            .await?;
        let anime_context = AnimeMatchingContext {
            planned_episode_count: metadata.planned_episode_count.0,
            title_names: keywords,
            air_date: metadata.air_date.0.clone(),
        };
        let decision = entity.match_resource(
            resource,
            matched_rule.as_ref(),
            &anime_context,
            now,
        )?;

        match decision {
            MatchDecision::Ready {
                updated_subscription,
                record,
            } => {
                let relative_save_path =
                    build_relative_save_path(&metadata);
                let download_result = (self.run_matched_resource)(MatchedResource {
                    user_id: subscription.user_id,
                    source_url: resource.source_url.clone(),
                    resource_id: resource.id.0.clone(),
                    relative_save_path,
                })
                .await;

                if let Err(error) = download_result {
                    tracing::error!(
                        user_id = %subscription.user_id.0,
                        anime_id = %subscription.anime_id.0,
                        resource_id = %resource.id.0,
                        resource_title = %resource.title,
                        matched_rule = %matched_rule.as_ref().map(|r| r.name.as_str()).unwrap_or("-"),
                        rule_pattern = %matched_rule.as_ref().map(|r| r.pattern.as_str()).unwrap_or("-"),
                        ?error,
                        "download matched resource failed, will retry on next match cycle"
                    );
                    return Ok(false);
                }

                if entity
                    .apply_match(
                        MatchDecision::Ready {
                            updated_subscription,
                            record,
                        },
                        &*self.subscriptions.caps.match_writer,
                    )
                    .await?
                    .is_none()
                {
                    return Ok(false);
                }
                self.subscriptions.save(&entity).await?;

                tracing::info!(
                    anime_id = %subscription.anime_id.0,
                    resource_id = %resource.id.0,
                    resource_title = %resource.title,
                    matched_rule = %matched_rule.as_ref().map(|r| r.name.as_str()).unwrap_or("-"),
                    "resource matched and download initiated"
                );

                Ok(true)
            }
            MatchDecision::Skip(reason) => {
                tracing::debug!(
                    %reason,
                    resource_id = %resource.id.0,
                    resource_title = %resource.title,
                    anime_id = %subscription.anime_id.0,
                    "apply_resource_to_subscription: match skipped"
                );
                Ok(false)
            }
        }
    }

    async fn resource_visible_to_subscription(
        &self,
        space_id: SpaceId,
        resource: &Resource,
    ) -> Result<bool, ApplicationError> {
        let _ = (space_id, resource);
        Ok(true)
    }

    async fn search_sources_for_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<FeedEntity>, ApplicationError> {
        let mut sources = Vec::new();

        for source in self
            .feeds
            .list(FeedListQuery {
                space_id: Some(space_id),
                with_site_url: false,
                with_search_url: true,
            })
            .await?
        {
            sources.push(source);
        }

        Ok(sources)
    }

    async fn match_space_rule(
        &self,
        space_id: SpaceId,
        title: &str,
    ) -> Result<Option<domain::rule::MatchingRule>, ApplicationError> {
        let provider = self.rules.regex_provider();
        for rule in self.rules.list(space_id).await? {
            if let Some(matched) = rule.match_title(provider, title)? {
                return Ok(Some(matched));
            }
        }
        Ok(None)
    }

    async fn match_subscription_rule(
        &self,
        subscription: &SubscriptionAnime,
        title: &str,
    ) -> Result<Option<domain::rule::MatchingRule>, ApplicationError> {
        let Some(bound_rule_name) = subscription.bound_rule_name.as_deref() else {
            return self.match_space_rule(subscription.space_id, title).await;
        };
        let Some(rule) = self
            .rules
            .find_by_name_including_inactive(subscription.space_id, bound_rule_name)
            .await?
        else {
            return Ok(None);
        };
        let matched = rule.match_title(self.rules.regex_provider(), title)?;
        tracing::debug!(
            bound_rule = %subscription.bound_rule_name.as_deref().unwrap_or("none"),
            %title,
            matched = matched.is_some(),
            "match_subscription_rule"
        );
        Ok(matched)
    }

    /// 为所有启用了自动订阅的空间，批量创建指定番剧的订阅（如尚未订阅）。
    pub async fn auto_subscribe_new_animes(
        &self,
        new_anime_ids: &[AnimeId],
    ) -> Result<(), ApplicationError> {
        if new_anime_ids.is_empty() {
            return Ok(());
        }

        let spaces = self.spaces.list_auto_subscribing_spaces().await?;
        let space_ids: Vec<SpaceId> = spaces.iter().map(|s| s.read_data().id).collect();
        let user_map = self.spaces.find_personal_space_user_ids(&space_ids).await?;

        for space in &spaces {
            let space_id = space.read_data().id;
            let Some(&user_id) = user_map.get(&space_id) else { continue };

            let decided: Vec<AnimeId> = new_anime_ids
                .iter()
                .copied()
                .filter_map(|id| space.try_auto_subscribe(id).map(|d| d.anime_id))
                .collect();
            if decided.is_empty() {
                continue;
            }

            let existing = self.subscriptions.list_anime_ids_in_space(space_id).await?;

            let to_create: Vec<AnimeId> = decided
                .into_iter()
                .filter(|id| !existing.contains(id))
                .collect();
            if to_create.is_empty() {
                continue;
            }

            let entities: Vec<SubscriptionAnimeEntity> = to_create
                .iter()
                .map(|&anime_id| {
                    SubscriptionAnimeEntity::new(
                        SubscriptionAnime {
                            user_id,
                            space_id,
                            anime_id,
                            enabled: true,
                            bound_rule_name: None,
                            search_state: SubscriptionSearchState::Stopped,
                            progress: 0,
                        },
                        vec![],
                    )
                    .expect("valid auto_subscribe entity")
                })
                .collect();
            self.subscriptions.save_list(&entities).await?;
            for anime_id in &to_create {
                self.ensure_anime_search_pool(user_id, space_id, *anime_id)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn batch_update_search_state_by_anime(
        &self,
        anime_id: AnimeId,
        state: SubscriptionSearchState,
    ) -> Result<(), ApplicationError> {
        let subscriptions = self.subscriptions.list_by_anime(anime_id).await?;
        let pks: Vec<SubscriptionPk> = subscriptions
            .into_iter()
            .map(|s| {
                let data = s.into_snapshot();
                (data.user_id, data.space_id, data.anime_id)
            })
            .collect();
        if pks.is_empty() {
            return Ok(());
        }
        self.subscriptions
            .batch_write_search_state(&pks, state)
            .await?;
        Ok(())
    }

    pub async fn get_search_pool_stats(&self) -> Result<(i64, i64), ApplicationError> {
        let searching_anime_count = self.search_pool.count_distinct_anime().await?;
        let pending_link_count = self.search_pool.count_pending_links().await?;
        Ok((searching_anime_count, pending_link_count))
    }
}

fn render_search_url(template: &str, keyword: &str) -> String {
    if template.contains("{}") {
        template.replacen("{}", keyword, 1)
    } else if template.contains("{0}") {
        template.replacen("{0}", keyword, 1)
    } else {
        template.to_string()
    }
}
