use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use crate::{shared::error::ApplicationError, subscription::service::SubscriptionService};

use super::Job;

pub struct CheckMissingEpisodesJob {
    service: Arc<SubscriptionService>,
}

impl CheckMissingEpisodesJob {
    pub fn new(service: Arc<SubscriptionService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Job for CheckMissingEpisodesJob {
    fn name(&self) -> &'static str {
        "check_missing_episodes"
    }

    async fn run(&self) -> Result<(), ApplicationError> {
        let outcome = self.service.check_missing_episodes().await?;
        info!(
            "check_missing_episodes completed: checked={}, resumed={}",
            outcome.checked_subscription_count, outcome.resumed_anime_count
        );
        Ok(())
    }
}
