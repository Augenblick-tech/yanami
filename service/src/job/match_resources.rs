use std::sync::Arc;

use async_trait::async_trait;

use crate::{shared::error::ApplicationError, subscription::service::SubscriptionService};

use super::Job;

pub struct MatchResourcesJob {
    service: Arc<SubscriptionService>,
}

impl MatchResourcesJob {
    pub fn new(service: Arc<SubscriptionService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Job for MatchResourcesJob {
    fn name(&self) -> &'static str {
        "match_resources"
    }

    async fn run(&self) -> Result<(), ApplicationError> {
        self.service.match_resources().await?;
        Ok(())
    }
}
