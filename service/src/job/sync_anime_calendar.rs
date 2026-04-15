use std::sync::Arc;

use anime::source::AnimeSourceFactory;
use async_trait::async_trait;
use tracing::info;

use crate::{
    anime::service::AnimeService, job::Job, shared::error::ApplicationError,
    subscription::service::SubscriptionService,
};

pub struct SyncAnimeCalendarJob {
    service: Arc<AnimeService>,
    subscription_service: Arc<SubscriptionService>,
    source_factory: Arc<AnimeSourceFactory>,
}

impl SyncAnimeCalendarJob {
    pub fn new(
        service: Arc<AnimeService>,
        subscription_service: Arc<SubscriptionService>,
        source_factory: Arc<AnimeSourceFactory>,
    ) -> Self {
        Self {
            service,
            subscription_service,
            source_factory,
        }
    }
}

#[async_trait]
impl Job for SyncAnimeCalendarJob {
    fn name(&self) -> &'static str {
        "sync_anime_calendar"
    }

    async fn run(&self) -> Result<(), ApplicationError> {
        let sources = (self.source_factory)()?;
        let mut fetched = 0usize;
        let mut persisted = 0usize;
        for source in sources {
            info!(source = %source.name(), "sync_anime_calendar source started");
            match self.service.sync_source(source.as_ref()).await {
                Ok(outcome) => {
                    fetched += outcome.fetched;
                    persisted += outcome.persisted;
                    if !outcome.new_anime_ids.is_empty() {
                        if let Err(error) = self
                            .subscription_service
                            .auto_subscribe_new_animes(&outcome.new_anime_ids)
                            .await
                        {
                            tracing::error!(
                                source = %source.name(),
                                ?error,
                                "auto_subscribe_new_animes failed"
                            );
                        }
                    }
                    info!(
                        source = %source.name(),
                        fetched = outcome.fetched,
                        persisted = outcome.persisted,
                        "sync_anime_calendar source completed"
                    );
                }
                Err(error) => {
                    tracing::error!(
                        source = %source.name(),
                        ?error,
                        "sync_anime_calendar source failed, skipping"
                    );
                }
            }
        }
        info!(
            "sync_anime_calendar completed: fetched={}, persisted={}",
            fetched, persisted
        );
        Ok(())
    }
}
