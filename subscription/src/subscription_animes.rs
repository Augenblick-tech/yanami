use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use domain::{
    anime::AnimeId,
    shared::{biz::BizContext, error::DomainError},
    space::SpaceId,
    subscription::{
        capability::{
            SubscriptionMatchCap, SubscriptionPk, SubscriptionSearchCap, SubscriptionToggleCap,
        },
        MatchRecordRepository, SubscriptionAnime, SubscriptionAnimeRepository,
        SubscriptionSearchState,
    },
    user::UserId,
};

use crate::{entity::SubscriptionAnimeEntity, shared::error::ApplicationError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionAnimeListQuery {
    pub space_id: SpaceId,
    pub anime_ids: Option<Vec<AnimeId>>,
    pub enabled: Option<bool>,
    pub search_state: Option<SubscriptionSearchState>,
}

#[derive(Clone)]
pub struct SubscriptionCaps {
    pub toggle: Arc<dyn SubscriptionToggleCap>,
    pub match_writer: Arc<dyn SubscriptionMatchCap>,
    pub search: Arc<dyn SubscriptionSearchCap>,
}

pub struct SubscriptionAnimes {
    pub caps: SubscriptionCaps,
    subscription_repository: Arc<dyn SubscriptionAnimeRepository>,
    match_record_repository: Arc<dyn MatchRecordRepository>,
}

impl SubscriptionAnimes {
    pub fn new(
        caps: SubscriptionCaps,
        subscription_repository: Arc<dyn SubscriptionAnimeRepository>,
        match_record_repository: Arc<dyn MatchRecordRepository>,
    ) -> Self {
        Self {
            caps,
            subscription_repository,
            match_record_repository,
        }
    }

    pub async fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            caps: SubscriptionCaps {
                toggle: self.caps.toggle.with_biz(biz).await?,
                match_writer: self.caps.match_writer.with_biz(biz).await?,
                search: self.caps.search.with_biz(biz).await?,
            },
            subscription_repository: self.subscription_repository.with_biz(biz)?,
            match_record_repository: self.match_record_repository.with_biz(biz)?,
        })
    }

    pub async fn create(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        enabled: bool,
    ) -> Result<SubscriptionAnimeEntity, ApplicationError> {
        let entity = self.build_entity(
            SubscriptionAnime {
                user_id,
                space_id,
                anime_id,
                enabled,
                bound_rule_name: None,
                search_state: SubscriptionSearchState::Stopped,
                progress: 0,
            },
            vec![],
        )?;
        self.subscription_repository
            .save_subscription(entity.read_data())
            .await?;
        Ok(entity)
    }

    pub async fn remove(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), ApplicationError> {
        self.subscription_repository
            .delete_subscription(user_id, space_id, anime_id)
            .await?;
        Ok(())
    }

    pub async fn load(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnimeEntity>, ApplicationError> {
        self.load_entity(user_id, space_id, anime_id).await
    }

    pub async fn list(
        &self,
        query: SubscriptionAnimeListQuery,
    ) -> Result<Vec<SubscriptionAnimeEntity>, ApplicationError> {
        let subscriptions = self
            .subscription_repository
            .list_subscriptions(query.space_id)
            .await?;
        let mut records_by_subscription = self
            .match_record_repository
            .list_space_match_records(query.space_id)
            .await?
            .into_iter()
            .fold(
                HashMap::<(UserId, AnimeId), Vec<domain::subscription::MatchRecord>>::new(),
                |mut grouped, record| {
                    grouped
                        .entry((record.user_id, record.anime_id))
                        .or_default()
                        .push(record);
                    grouped
                },
            );
        let anime_id_filter = query
            .anime_ids
            .map(|anime_ids| anime_ids.into_iter().collect::<HashSet<_>>());
        let mut entities = Vec::new();

        for subscription in subscriptions {
            if query
                .enabled
                .is_some_and(|enabled| subscription.enabled != enabled)
            {
                continue;
            }
            if query
                .search_state
                .is_some_and(|state| subscription.search_state != state)
            {
                continue;
            }
            if anime_id_filter
                .as_ref()
                .is_some_and(|anime_ids| !anime_ids.contains(&subscription.anime_id))
            {
                continue;
            }
            let records = records_by_subscription
                .remove(&(subscription.user_id, subscription.anime_id))
                .unwrap_or_default();
            entities.push(self.build_entity(subscription, records)?);
        }

        Ok(entities)
    }

    pub async fn list_enabled_in_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnimeEntity>, ApplicationError> {
        self.subscription_repository
            .list_enabled_subscriptions(space_id)
            .await?
            .into_iter()
            .map(|subscription| self.build_entity(subscription, vec![]).map_err(Into::into))
            .collect()
    }

    pub async fn list_enabled(&self) -> Result<Vec<SubscriptionAnimeEntity>, ApplicationError> {
        self.subscription_repository
            .list_all_enabled_subscriptions()
            .await?
            .into_iter()
            .map(|subscription| self.build_entity(subscription, vec![]).map_err(Into::into))
            .collect()
    }

    pub async fn pick_one_pending(
        &self,
    ) -> Result<Option<SubscriptionAnimeEntity>, ApplicationError> {
        let Some(subscription) = self.subscription_repository.pick_one_pending().await? else {
            return Ok(None);
        };
        Ok(Some(self.build_entity(subscription, vec![])?))
    }

    pub async fn pick_one_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnimeEntity>, ApplicationError> {
        let Some(subscription) = self.subscription_repository.pick_one_localmatch().await? else {
            return Ok(None);
        };
        Ok(Some(self.build_entity(subscription, vec![])?))
    }

    pub async fn pick_one_pending_or_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnimeEntity>, ApplicationError> {
        let Some(subscription) = self
            .subscription_repository
            .pick_one_pending_or_localmatch()
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.build_entity(subscription, vec![])?))
    }

    pub async fn list_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnimeEntity>, ApplicationError> {
        let subscriptions = self
            .subscription_repository
            .list_subscriptions_by_anime(anime_id)
            .await?;
        subscriptions
            .into_iter()
            .map(|subscription| self.build_entity(subscription, vec![]).map_err(Into::into))
            .collect()
    }

    pub async fn list_all_in_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnimeEntity>, ApplicationError> {
        self.subscription_repository
            .list_subscriptions(space_id)
            .await?
            .into_iter()
            .map(|subscription| self.build_entity(subscription, vec![]).map_err(Into::into))
            .collect()
    }

    /// 轻量批量查询：该空间下已订阅的番剧 ID 集合。不加载 match_record。
    pub async fn list_anime_ids_in_space(
        &self,
        space_id: SpaceId,
    ) -> Result<HashSet<AnimeId>, ApplicationError> {
        let ids = self
            .subscription_repository
            .list_subscription_anime_ids_by_space(space_id)
            .await?;
        Ok(ids.into_iter().collect())
    }

    /// 批量写入搜索状态。绕过 entity（按 anime_id 批量操作时不需要加载每个 entity）。
    pub async fn batch_write_search_state(
        &self,
        pks: &[SubscriptionPk],
        state: SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        self.caps.search.batch_write_search_state(pks, state).await
    }

    pub async fn save(&self, entity: &SubscriptionAnimeEntity) -> Result<(), ApplicationError> {
        self.subscription_repository
            .save_subscription(entity.read_data())
            .await?;
        for record in entity.read_records() {
            self.match_record_repository
                .save_match_record(record)
                .await?;
        }
        Ok(())
    }

    pub async fn save_list(
        &self,
        entities: &[SubscriptionAnimeEntity],
    ) -> Result<(), ApplicationError> {
        let subs: Vec<&SubscriptionAnime> = entities.iter().map(|e| e.read_data()).collect();
        self.subscription_repository
            .save_subscription_batch(&subs)
            .await?;
        for entity in entities {
            for record in entity.read_records() {
                self.match_record_repository
                    .save_match_record(record)
                    .await?;
            }
        }
        Ok(())
    }

    fn build_entity(
        &self,
        subscription: SubscriptionAnime,
        records: Vec<domain::subscription::MatchRecord>,
    ) -> Result<SubscriptionAnimeEntity, DomainError> {
        SubscriptionAnimeEntity::new(subscription, records)
    }

    async fn load_entity(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnimeEntity>, ApplicationError> {
        let Some(subscription) = self
            .subscription_repository
            .find_subscription(user_id, space_id, anime_id)
            .await?
        else {
            return Ok(None);
        };
        let records = self
            .match_record_repository
            .list_match_records(user_id, space_id, anime_id)
            .await?;
        Ok(Some(self.build_entity(subscription, records)?))
    }
}
