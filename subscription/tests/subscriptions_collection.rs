use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{
    anime::AnimeId,
    shared::error::DomainError,
    space::SpaceId,
    subscription::{
        capability::{SubscriptionMatchCap, SubscriptionSearchCap, SubscriptionToggleCap},
        LatestMatchRecord, MatchRecord, MatchRecordRepository, MatchResourceId, SubscriptionAnime,
        SubscriptionAnimeRepository, SubscriptionSearchState,
    },
    user::UserId,
};
use subscription::{SubscriptionAnimes, SubscriptionCaps};

#[derive(Default)]
struct State {
    subscriptions: HashMap<(UserId, SpaceId, AnimeId), SubscriptionAnime>,
    records: Vec<MatchRecord>,
}

struct NoopSubscriptions {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl SubscriptionAnimeRepository for NoopSubscriptions {
    async fn find_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .get(&(user_id, space_id, anime_id))
            .cloned())
    }

    async fn list_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
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
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .filter(|subscription| subscription.space_id == space_id && subscription.enabled)
            .cloned()
            .collect())
    }

    async fn list_all_enabled_subscriptions(&self) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .filter(|subscription| subscription.enabled)
            .cloned()
            .collect())
    }

    async fn list_pending_search_subscriptions(
        &self,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .filter(|subscription| {
                subscription.enabled
                    && subscription.search_state
                        == domain::subscription::SubscriptionSearchState::Pending
            })
            .cloned()
            .collect())
    }

    async fn list_subscriptions_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .filter(|s| s.anime_id == anime_id)
            .cloned()
            .collect())
    }

    async fn has_enabled_subscription(&self, anime_id: AnimeId) -> Result<bool, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .any(|subscription| subscription.anime_id == anime_id && subscription.enabled))
    }

    async fn save_subscription(&self, subscription: &SubscriptionAnime) -> Result<(), DomainError> {
        self.state.lock().expect("state").subscriptions.insert(
            (
                subscription.user_id,
                subscription.space_id,
                subscription.anime_id,
            ),
            subscription.clone(),
        );
        Ok(())
    }

    async fn save_subscription_batch(&self, subscriptions: &[&SubscriptionAnime]) -> Result<(), DomainError> {
        let mut state = self.state.lock().expect("state");
        for subscription in subscriptions {
            state.subscriptions.insert(
                (subscription.user_id, subscription.space_id, subscription.anime_id),
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
        self.state
            .lock()
            .expect("state")
            .subscriptions
            .remove(&(user_id, space_id, anime_id));
        Ok(())
    }

    async fn list_subscription_anime_ids_by_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<AnimeId>, DomainError> {
        Ok(self
            .state
            .lock()
            .expect("state")
            .subscriptions
            .values()
            .filter(|s| s.space_id == space_id)
            .map(|s| s.anime_id)
            .collect())
    }
}

struct NoopRecords {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl MatchRecordRepository for NoopRecords {
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

    async fn save_match_record(&self, record: &MatchRecord) -> Result<(), DomainError> {
        self.state
            .lock()
            .expect("state")
            .records
            .push(record.clone());
        Ok(())
    }
}

struct NoopToggle;
#[async_trait]
impl SubscriptionToggleCap for NoopToggle {
    async fn write_enabled(
        &self,
        _pk: (UserId, SpaceId, AnimeId),
        _enabled: bool,
    ) -> Result<(), DomainError> { Ok(()) }
}

struct NoopMatch;
#[async_trait]
impl SubscriptionMatchCap for NoopMatch {
    async fn write_match_result(
        &self,
        _pk: (UserId, SpaceId, AnimeId),
        _progress: i64,
        _bound_rule: Option<String>,
        _enabled: bool,
    ) -> Result<(), DomainError> { Ok(()) }
}

struct NoopSearch;
#[async_trait]
impl SubscriptionSearchCap for NoopSearch {
    async fn write_search_state(
        &self,
        _pk: (UserId, SpaceId, AnimeId),
        _state: SubscriptionSearchState,
    ) -> Result<(), DomainError> { Ok(()) }

    async fn batch_write_search_state(
        &self,
        _pks: &[(UserId, SpaceId, AnimeId)],
        _state: SubscriptionSearchState,
    ) -> Result<(), DomainError> { Ok(()) }
}

#[tokio::test]
async fn constructs_collection_and_exposes_runtime() {
    let state = Arc::new(Mutex::new(State::default()));
    let caps = SubscriptionCaps {
        toggle: Arc::new(NoopToggle),
        match_writer: Arc::new(NoopMatch),
        search: Arc::new(NoopSearch),
    };
    let collection = SubscriptionAnimes::new(
        caps,
        Arc::new(NoopSubscriptions {
            state: state.clone(),
        }),
        Arc::new(NoopRecords { state }),
    );

    collection
        .create(UserId(1), SpaceId(1), AnimeId(7), true)
        .await
        .expect("create subscription");
    let loaded = collection
        .load(UserId(1), SpaceId(1), AnimeId(7))
        .await
        .expect("load subscription")
        .expect("subscription");

    assert!(loaded.read_data().enabled);
}
