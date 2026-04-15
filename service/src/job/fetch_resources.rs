use std::sync::Arc;

use async_trait::async_trait;
use tracing::info;

use crate::{shared::error::ApplicationError, subscription::service::SubscriptionService};

use super::Job;

pub struct FetchResourcesJob {
    service: Arc<SubscriptionService>,
}

impl FetchResourcesJob {
    pub fn new(service: Arc<SubscriptionService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Job for FetchResourcesJob {
    fn name(&self) -> &'static str {
        "fetch_resources"
    }

    async fn run(&self) -> Result<(), ApplicationError> {
        let fetched = self.service.fetch_resources().await?;
        info!(
            "fetch_resources completed: saved_count={}",
            fetched.saved_count
        );

        let matched = self.service.match_resources().await?;
        info!(
            "match_resources completed: resource_count={}, matched_count={}",
            matched.resource_count, matched.matched_count
        );

        Ok(())
    }
}
