use std::sync::Arc;

use common::shared::error::Error;
use feed::entity::{
    cap::{FeedAccessPolicy, FeedFetcher},
    model::{FeedFetchError, FeedFetchResult},
};

use crate::entity::model::SearchMandateBaseData;

#[derive(Clone)]
pub struct SearchMandateEntity {
    data: SearchMandateBaseData,
    fetch_cap: Arc<dyn FeedFetcher>,
    access_policy: Arc<dyn FeedAccessPolicy>,
    completed: bool,
}

impl SearchMandateEntity {
    pub(super) fn new(
        data: SearchMandateBaseData,
        fetch_cap: Arc<dyn FeedFetcher>,
        access_policy: Arc<dyn FeedAccessPolicy>,
    ) -> Self {
        Self {
            data,
            fetch_cap,
            access_policy,
            completed: false,
        }
    }
}

impl SearchMandateEntity {
    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn anime_id(&self) -> i64 {
        self.data.mandata.anime_id
    }
}

impl SearchMandateEntity {
    pub async fn fetch(&mut self) -> Result<FeedFetchResult, Error> {
        if !self.access_policy.is_access(self.data.mandata.feed_id) {
            return Ok(FeedFetchResult::Denied);
        }

        let data = self.fetch_cap.fetch_url(&self.data.mandata.url).await;
        let res = match data {
            Ok(data) => {
                self.completed = true;
                Ok(FeedFetchResult::Success(data.items))
            }
            Err(FeedFetchError::Inaccessible(v)) => {
                self.completed = true;
                Ok(FeedFetchResult::Failure(Error::conflict(v)))
            }
            Err(FeedFetchError::Retryable(v)) => Ok(FeedFetchResult::Retryable(Error::conflict(v))),
            Err(e) => Err(Error::external(
                format!(
                    "search mandate {} fetch {} failed",
                    self.id(),
                    &self.data.mandata.url
                ),
                e,
            )),
        };
        if let Ok(res) = &res {
            self.access_policy.note(self.data.mandata.feed_id, res);
        }

        res
    }
}
