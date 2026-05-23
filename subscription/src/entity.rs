use chrono::{DateTime, Days, NaiveDate};
use domain::{
    feed::Resource,
    rule::MatchingRule,
    shared::error::DomainError,
    space::SpaceId,
    subscription::{
        capability::{SubscriptionMatchCap, SubscriptionSearchCap, SubscriptionToggleCap},
        AnimeMatchingContext, AnimeProgressState, MatchDecision, MatchRecord, MatchResourceId,
        SkipReason, SubscriptionAnime, SubscriptionSearchState,
    },
};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone)]
pub struct SubscriptionAnimeEntity {
    subscription: SubscriptionAnime,
    records: Vec<MatchRecord>,
}

impl SubscriptionAnimeEntity {
    pub fn new(
        subscription: SubscriptionAnime,
        records: Vec<MatchRecord>,
    ) -> Result<Self, DomainError> {
        for record in &records {
            validate_record(&subscription, record)?;
        }
        Ok(Self {
            subscription,
            records,
        })
    }

    pub fn read_data(&self) -> &SubscriptionAnime {
        &self.subscription
    }

    pub fn read_records(&self) -> &[MatchRecord] {
        &self.records
    }

    pub fn into_snapshot(self) -> SubscriptionAnime {
        self.subscription
    }

    pub async fn disable(&mut self, toggle: &dyn SubscriptionToggleCap) -> Result<(), DomainError> {
        if !self.subscription.enabled {
            return Ok(());
        }
        let pk = self.pk();
        toggle.write_enabled(pk, false).await?;
        self.subscription.enabled = false;
        Ok(())
    }

    pub async fn enable(&mut self, toggle: &dyn SubscriptionToggleCap) -> Result<(), DomainError> {
        if self.subscription.enabled {
            return Ok(());
        }
        let pk = self.pk();
        toggle.write_enabled(pk, true).await?;
        self.subscription.enabled = true;
        Ok(())
    }

    pub async fn resume_if_completed(
        &mut self,
        planned_episode_count: i64,
        toggle: &dyn SubscriptionToggleCap,
    ) -> Result<bool, DomainError> {
        if self.subscription.enabled {
            return Ok(false);
        }
        if self.subscription.progress > 0 && self.subscription.progress < planned_episode_count {
            let pk = self.pk();
            toggle.write_enabled(pk, true).await?;
            self.subscription.enabled = true;
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn start_search(
        &mut self,
        search: &dyn SubscriptionSearchCap,
    ) -> Result<(), DomainError> {
        if self.subscription.search_state != SubscriptionSearchState::Stopped {
            return Ok(());
        }
        let pk = self.pk();
        search.write_search_state(pk, SubscriptionSearchState::Pending).await?;
        self.subscription.search_state = SubscriptionSearchState::Pending;
        Ok(())
    }

    pub async fn stop_search(
        &mut self,
        search: &dyn SubscriptionSearchCap,
    ) -> Result<(), DomainError> {
        if self.subscription.search_state == SubscriptionSearchState::Stopped {
            return Ok(());
        }
        let pk = self.pk();
        search.write_search_state(pk, SubscriptionSearchState::Stopped).await?;
        self.subscription.search_state = SubscriptionSearchState::Stopped;
        Ok(())
    }

    pub async fn mark_search_running(
        &mut self,
        search: &dyn SubscriptionSearchCap,
    ) -> Result<(), DomainError> {
        if self.subscription.search_state != SubscriptionSearchState::Pending {
            return Ok(());
        }
        let pk = self.pk();
        search.write_search_state(pk, SubscriptionSearchState::Running).await?;
        self.subscription.search_state = SubscriptionSearchState::Running;
        Ok(())
    }

    pub async fn resume_search(
        &mut self,
        search: &dyn SubscriptionSearchCap,
        missing_count: i64,
    ) -> Result<(), DomainError> {
        if self.subscription.search_state != SubscriptionSearchState::Stopped {
            return Ok(());
        }
        if missing_count <= 0 || missing_count > 5 {
            return Ok(());
        }
        let pk = self.pk();
        search.write_search_state(pk, SubscriptionSearchState::Pending).await?;
        self.subscription.search_state = SubscriptionSearchState::Pending;
        Ok(())
    }

    pub fn move_to_space(&mut self, space_id: SpaceId) {
        if self.subscription.space_id == space_id {
            return;
        }
        self.subscription.space_id = space_id;
        self.records.clear();
    }

    /// 搜索池耗尽后决定目标状态。
    /// remaining == 0 → Stopped，否则 Pending。
    pub fn decide_search_target_state(remaining: i64) -> SubscriptionSearchState {
        if remaining == 0 {
            SubscriptionSearchState::Stopped
        } else {
            SubscriptionSearchState::Pending
        }
    }

    pub fn progress_state(&self, planned_episode_count: i64) -> AnimeProgressState {
        match self.subscription.progress {
            p if p >= planned_episode_count => AnimeProgressState::Completed,
            0 => AnimeProgressState::NotStarted,
            _ => AnimeProgressState::InProgress,
        }
    }

    pub fn match_resource(
        &self,
        resource: &Resource,
        matched_rule: Option<&MatchingRule>,
        anime_context: &AnimeMatchingContext,
        now: i64,
    ) -> Result<MatchDecision, DomainError> {
        if !self.subscription.enabled {
            return Ok(MatchDecision::Skip(SkipReason::SubscriptionDisabled));
        }
        if !title_matches_subscription(&resource.title, &anime_context.title_names) {
            return Ok(MatchDecision::Skip(SkipReason::TitleMismatch));
        }

        let Some(matched_rule) = matched_rule else {
            return Ok(MatchDecision::Skip(SkipReason::NoMatchingRule));
        };

        if !is_release_date_valid(resource.published_at, &anime_context.air_date)? {
            return Ok(MatchDecision::Skip(SkipReason::ReleaseDateInvalid));
        }

        if let Some(bound_rule_name) = &self.subscription.bound_rule_name {
            if bound_rule_name != &matched_rule.name {
                return Ok(MatchDecision::Skip(SkipReason::BoundRuleMismatch {
                    bound: bound_rule_name.clone(),
                    matched: matched_rule.name.clone(),
                }));
            }
        }

        let resource_id = MatchResourceId(resource.id.0.clone());
        if self
            .records
            .iter()
            .any(|record| record.resource_id == resource_id)
        {
            return Ok(MatchDecision::Skip(SkipReason::AlreadyMatched));
        }

        let record = MatchRecord {
            user_id: self.subscription.user_id,
            space_id: self.subscription.space_id,
            anime_id: self.subscription.anime_id,
            resource_id,
            title: resource.title.clone(),
            source_url: resource.source_url.clone(),
            matched_rule_name: matched_rule.name.clone(),
            published_at: resource.published_at,
            created_at: now,
        };

        let mut updated_subscription = self.subscription.clone();
        if updated_subscription.bound_rule_name.is_none() {
            updated_subscription.bound_rule_name = Some(matched_rule.name.clone());
        }

        let mut titles = self
            .records
            .iter()
            .map(|record| record.title.clone())
            .collect::<Vec<_>>();
        titles.push(record.title.clone());
        let progress = derive_progress_from_titles(&titles);
        if progress > updated_subscription.progress {
            updated_subscription.progress = progress;
        }
        if updated_subscription.progress >= anime_context.planned_episode_count {
            updated_subscription.enabled = false;
        }

        Ok(MatchDecision::Ready {
            updated_subscription: Box::new(updated_subscription),
            record: Box::new(record),
        })
    }

    pub async fn apply_match(
        &mut self,
        decision: MatchDecision,
        writer: &dyn SubscriptionMatchCap,
    ) -> Result<Option<MatchRecord>, DomainError> {
        let MatchDecision::Ready {
            updated_subscription,
            record,
        } = decision
        else {
            return Ok(None);
        };
        let pk = self.pk();

        writer
            .write_match_result(
                pk,
                updated_subscription.progress,
                updated_subscription.bound_rule_name.clone(),
                updated_subscription.enabled,
            )
            .await?;
        self.records.push((*record).clone());
        self.subscription = *updated_subscription;
        Ok(Some(*record))
    }

    fn pk(&self) -> (domain::user::UserId, SpaceId, domain::anime::AnimeId) {
        (self.subscription.user_id, self.subscription.space_id, self.subscription.anime_id)
    }
}

fn validate_record(
    subscription: &SubscriptionAnime,
    record: &MatchRecord,
) -> Result<(), DomainError> {
    if record.user_id != subscription.user_id
        || record.space_id != subscription.space_id
        || record.anime_id != subscription.anime_id
    {
        return Err(DomainError::InvariantViolation(
            "subscription record does not belong to subscription",
        ));
    }
    Ok(())
}

fn title_matches_subscription(title: &str, anime_title_names: &[String]) -> bool {
    let normalized_title = normalize_text(title);
    let candidates: Vec<String> = anime_title_names
        .iter()
        .map(|candidate| normalize_text(candidate))
        .collect();
    let matched = candidates
        .iter()
        .any(|candidate| !candidate.is_empty() && normalized_title.contains(candidate));
    tracing::debug!(
        %normalized_title,
        ?candidates,
        matched,
        "title_matches_subscription"
    );
    matched
}

fn normalize_text(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .chars()
        .filter(|char| !char.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

fn is_release_date_valid(published_at: Option<i64>, air_date: &str) -> Result<bool, DomainError> {
    let Some(published_at) = published_at else {
        return Ok(true);
    };
    let Some(published_at) = DateTime::from_timestamp(published_at, 0) else {
        return Ok(false);
    };
    let Ok(air_date) = NaiveDate::parse_from_str(air_date, "%Y-%m-%d") else {
        return Ok(false);
    };

    let valid = published_at
        .date_naive()
        .checked_add_days(Days::new(30))
        .is_some_and(|deadline| deadline >= air_date);
    tracing::debug!(
        ?published_at,
        %air_date,
        valid,
        "is_release_date_valid"
    );
    Ok(valid)
}

fn derive_progress_from_titles(titles: &[String]) -> i64 {
    crate::episode_extractor::extract_episode_numbers(titles).len() as i64
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use domain::{
        anime::AnimeId,
        space::SpaceId,
        subscription::{
            capability::{SubscriptionMatchCap, SubscriptionSearchCap, SubscriptionToggleCap},
            SubscriptionAnime, SubscriptionSearchState,
        },
        user::UserId,
    };

    use super::*;

    struct NoopToggle;
    #[async_trait]
    impl SubscriptionToggleCap for NoopToggle {
        async fn write_enabled(
            &self,
            _pk: (UserId, SpaceId, AnimeId),
            _enabled: bool,
        ) -> Result<(), DomainError> {
            Ok(())
        }
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
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct NoopSearch;
    #[async_trait]
    impl SubscriptionSearchCap for NoopSearch {
        async fn write_search_state(
            &self,
            _pk: (UserId, SpaceId, AnimeId),
            _state: SubscriptionSearchState,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn batch_write_search_state(
            &self,
            _pks: &[(UserId, SpaceId, AnimeId)],
            _state: SubscriptionSearchState,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn make_subscription_with_progress(enabled: bool, progress: i64) -> SubscriptionAnimeEntity {
        SubscriptionAnimeEntity::new(
            SubscriptionAnime {
                user_id: UserId(1),
                space_id: SpaceId(1),
                anime_id: AnimeId(1),
                enabled,
                bound_rule_name: None,
                search_state: SubscriptionSearchState::Stopped,
                progress,
            },
            vec![],
        )
        .expect("entity")
    }

    fn make_subscription(enabled: bool) -> SubscriptionAnimeEntity {
        make_subscription_with_progress(enabled, 0)
    }

    #[tokio::test]
    async fn resume_if_completed_disabled_with_progress_below_planned_reenables() {
        let mut entity = make_subscription_with_progress(false, 12);
        let changed = entity.resume_if_completed(24, &NoopToggle).await.expect("resume");
        assert!(changed);
        assert!(entity.read_data().enabled);
    }

    #[tokio::test]
    async fn resume_if_completed_already_enabled_does_nothing() {
        let mut entity = make_subscription_with_progress(true, 12);
        let changed = entity.resume_if_completed(24, &NoopToggle).await.expect("resume");
        assert!(!changed);
        assert!(entity.read_data().enabled);
    }

    #[tokio::test]
    async fn resume_if_completed_disabled_with_progress_equal_to_planned_does_nothing() {
        let mut entity = make_subscription_with_progress(false, 12);
        let changed = entity.resume_if_completed(12, &NoopToggle).await.expect("resume");
        assert!(!changed);
        assert!(!entity.read_data().enabled);
    }

    #[tokio::test]
    async fn resume_if_completed_disabled_with_progress_above_planned_does_nothing() {
        let mut entity = make_subscription_with_progress(false, 15);
        let changed = entity.resume_if_completed(12, &NoopToggle).await.expect("resume");
        assert!(!changed);
        assert!(!entity.read_data().enabled);
    }

    #[tokio::test]
    async fn resume_if_completed_zero_progress_not_reenabled() {
        let mut entity = make_subscription_with_progress(false, 0);
        let changed = entity.resume_if_completed(12, &NoopToggle).await.expect("resume");
        assert!(!changed);
        assert!(!entity.read_data().enabled);
    }

    #[tokio::test]
    async fn enable_changes_disabled_to_enabled() {
        let mut entity = make_subscription(false);
        entity.enable(&NoopToggle).await.expect("enable");
        assert!(entity.read_data().enabled);
    }

    #[tokio::test]
    async fn disable_changes_enabled_to_disabled() {
        let mut entity = make_subscription(true);
        entity.disable(&NoopToggle).await.expect("disable");
        assert!(!entity.read_data().enabled);
    }

    #[tokio::test]
    async fn enable_on_already_enabled_is_idempotent() {
        let mut entity = make_subscription(true);
        entity.enable(&NoopToggle).await.expect("enable");
        assert!(entity.read_data().enabled);
    }

    #[tokio::test]
    async fn disable_on_already_disabled_is_idempotent() {
        let mut entity = make_subscription(false);
        entity.disable(&NoopToggle).await.expect("disable");
        assert!(!entity.read_data().enabled);
    }

    #[tokio::test]
    async fn start_search_transitions_stopped_to_pending() {
        let mut entity = make_subscription(true);
        entity.start_search(&NoopSearch).await.expect("start_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Pending);
    }

    #[tokio::test]
    async fn stop_search_transitions_any_to_stopped() {
        let mut entity = make_subscription(true);
        entity.start_search(&NoopSearch).await.expect("start_search");
        entity.stop_search(&NoopSearch).await.expect("stop_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Stopped);
    }

    #[tokio::test]
    async fn start_search_when_not_stopped_is_idempotent() {
        let mut entity = make_subscription(true);
        entity.start_search(&NoopSearch).await.expect("start_search");
        entity.start_search(&NoopSearch).await.expect("start_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Pending);
    }

    #[test]
    fn release_date_within_30_days_returns_true() {
        let published_at = NaiveDate::from_ymd_opt(2026, 5, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let result = is_release_date_valid(Some(published_at), "2026-06-01");
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn release_date_beyond_30_days_returns_false() {
        let published_at = NaiveDate::from_ymd_opt(2026, 4, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let result = is_release_date_valid(Some(published_at), "2026-06-01");
        assert!(result.is_ok() && !result.unwrap());
    }

    #[test]
    fn release_date_exactly_30_days_returns_true() {
        let published_at = NaiveDate::from_ymd_opt(2026, 5, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let result = is_release_date_valid(Some(published_at), "2026-06-01");
        assert!(result.is_ok() && result.unwrap());
    }

    #[test]
    fn release_date_31_days_before_returns_false() {
        let published_at = NaiveDate::from_ymd_opt(2026, 5, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let result = is_release_date_valid(Some(published_at), "2026-06-01");
        assert!(result.is_ok() && !result.unwrap());
    }

    #[test]
    fn release_date_none_returns_true() {
        let result = is_release_date_valid(None, "2026-06-01");
        assert!(result.is_ok() && result.unwrap());
    }

    #[tokio::test]
    async fn resume_search_from_stopped_with_valid_count_resumes() {
        let mut entity = make_subscription(true);
        entity.resume_search(&NoopSearch, 3).await.expect("resume_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Pending);
    }

    #[tokio::test]
    async fn resume_search_from_running_is_idempotent() {
        let mut entity = make_subscription(true);
        entity.start_search(&NoopSearch).await.expect("start_search");
        entity.resume_search(&NoopSearch, 3).await.expect("resume_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Pending);
    }

    #[tokio::test]
    async fn resume_search_with_zero_missing_is_noop() {
        let mut entity = make_subscription(true);
        entity.resume_search(&NoopSearch, 0).await.expect("resume_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Stopped);
    }

    #[tokio::test]
    async fn resume_search_with_count_above_5_is_noop() {
        let mut entity = make_subscription(true);
        entity.resume_search(&NoopSearch, 6).await.expect("resume_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Stopped);
    }

    #[tokio::test]
    async fn resume_search_with_negative_count_is_noop() {
        let mut entity = make_subscription(true);
        entity.resume_search(&NoopSearch, -1).await.expect("resume_search");
        assert_eq!(entity.read_data().search_state, SubscriptionSearchState::Stopped);
    }

    #[tokio::test]
    async fn apply_match_with_ready_decision_updates_state() {
        use domain::subscription::{MatchDecision, MatchRecord, MatchResourceId};
        let mut entity = make_subscription(true);
        let decision = MatchDecision::Ready {
            updated_subscription: Box::new(SubscriptionAnime {
                user_id: UserId(1),
                space_id: SpaceId(1),
                anime_id: AnimeId(1),
                enabled: false,
                bound_rule_name: Some("rule1".into()),
                search_state: SubscriptionSearchState::Stopped,
                progress: 3,
            }),
            record: Box::new(MatchRecord {
                user_id: UserId(1),
                space_id: SpaceId(1),
                anime_id: AnimeId(1),
                resource_id: MatchResourceId("res1".into()),
                title: "ep 3".into(),
                source_url: "url".into(),
                matched_rule_name: "rule1".into(),
                published_at: None,
                created_at: 100,
            }),
        };
        let result = entity.apply_match(decision, &NoopMatch).await.expect("apply_match");
        assert!(result.is_some());
        assert_eq!(entity.read_data().progress, 3);
        assert!(!entity.read_data().enabled);
    }
}
