use std::sync::Arc;

use domain::feed::{FeedSource, FeedSourceId};
use feed::{FeedListQuery, Feeds as FeedRoot};

use crate::shared::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetFeedsOutcome {
    pub space_id: domain::space::SpaceId,
    pub sources: Vec<FeedSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFeedOutcome {
    pub space_id: domain::space::SpaceId,
    pub source: FeedSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteFeedOutcome {
    pub space_id: domain::space::SpaceId,
    pub source_id: FeedSourceId,
}

pub struct FeedService {
    feeds: Arc<FeedRoot>,
}

impl FeedService {
    pub fn new(feeds: Arc<FeedRoot>) -> Self {
        Self { feeds }
    }

    pub async fn get_feeds(
        &self,
        space_id: domain::space::SpaceId,
    ) -> Result<GetFeedsOutcome, ApplicationError> {
        let sources = self
            .feeds
            .list(FeedListQuery {
                space_id: Some(space_id),
                with_site_url: false,
                with_search_url: false,
            })
            .await?
            .into_iter()
            .map(|entity| entity.into_snapshot())
            .collect();
        Ok(GetFeedsOutcome { space_id, sources })
    }

    pub async fn save_feed(
        &self,
        space_id: domain::space::SpaceId,
        source: FeedSource,
    ) -> Result<SaveFeedOutcome, ApplicationError> {
        let entity = self.feeds.save_source(space_id, source).await?;
        Ok(SaveFeedOutcome {
            space_id,
            source: entity.into_snapshot(),
        })
    }

    pub async fn delete_feed(
        &self,
        space_id: domain::space::SpaceId,
        source_id: FeedSourceId,
    ) -> Result<DeleteFeedOutcome, ApplicationError> {
        self.feeds
            .delete_source(space_id, source_id.clone())
            .await?;
        Ok(DeleteFeedOutcome {
            space_id,
            source_id,
        })
    }
}
