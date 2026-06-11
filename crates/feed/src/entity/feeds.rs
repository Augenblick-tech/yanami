use std::sync::Arc;

use common::shared::error::Error;

use crate::entity::{
    cap::{FeedAccessPolicy, FeedFetcher, FeedRepository},
    feed_entity::FeedEntity,
    model::{FeedListQuery, FeedType},
    search_feeds::SearchFeeds,
};

#[derive(Clone)]
pub struct Feeds {
    repo: Arc<dyn FeedRepository>,
    fetch_cap: Arc<dyn FeedFetcher>,
    access_policy: Arc<dyn FeedAccessPolicy>,
}

impl Feeds {
    pub fn new(
        repo: Arc<dyn FeedRepository>,
        fetch_cap: Arc<dyn FeedFetcher>,
        access_policy: Arc<dyn FeedAccessPolicy>,
    ) -> Self {
        Self {
            repo,
            fetch_cap,
            access_policy,
        }
    }

    pub async fn list(&self) -> Result<Vec<FeedEntity>, Error> {
        Ok(self
            .repo
            .list(&FeedListQuery {
                feed_type: FeedType::Both,
            })
            .await
            .map_err(|e| Error::external("feeds list failed", e))?
            .into_iter()
            .map(|i| FeedEntity::new(i.data, self.fetch_cap.clone(), self.access_policy.clone()))
            .collect())
    }

    pub async fn list_site_feeds(&self) -> Result<Vec<FeedEntity>, Error> {
        Ok(self
            .repo
            .list(&FeedListQuery {
                feed_type: FeedType::Site,
            })
            .await
            .map_err(|e| Error::external("feeds list site feed failed", e))?
            .into_iter()
            .map(|i| FeedEntity::new(i.data, self.fetch_cap.clone(), self.access_policy.clone()))
            .collect())
    }

    pub async fn list_search_feeds(&self) -> Result<Vec<FeedEntity>, Error> {
        Ok(self
            .repo
            .list(&FeedListQuery {
                feed_type: FeedType::Search,
            })
            .await
            .map_err(|e| Error::external("feeds list search feed failed", e))?
            .into_iter()
            .map(|i| FeedEntity::new(i.data, self.fetch_cap.clone(), self.access_policy.clone()))
            .collect())
    }

    pub async fn get_search_feeds(&self) -> Result<SearchFeeds, Error> {
        let prop = self
            .repo
            .list(&FeedListQuery {
                feed_type: FeedType::Search,
            })
            .await
            .map_err(|e| Error::external("feeds get_search_feeds load data failed", e))?;
        Ok(SearchFeeds::new(prop))
    }

    pub async fn create(
        &self,
        title: String,
        site_url: Option<String>,
        search_url: Option<String>,
    ) -> Result<FeedEntity, Error> {
        let metadata =
            FeedEntity::verify_metadata(self.fetch_cap.as_ref(), title, site_url, search_url)
                .await?;
        let prop = self
            .repo
            .insert(&metadata)
            .await
            .map_err(|e| Error::external("feeds insert failed", e))?;
        Ok(FeedEntity::new(
            prop.data,
            self.fetch_cap.clone(),
            self.access_policy.clone(),
        ))
    }

    pub async fn save(&self, entity: &FeedEntity) -> Result<(), Error> {
        self.repo
            .update(entity.get_base_data())
            .await
            .map_err(|e| Error::external("feeds save failed", e))?;
        Ok(())
    }

    pub async fn get(&self, feed_id: i64) -> Result<Option<FeedEntity>, Error> {
        Ok(self
            .repo
            .get(feed_id)
            .await
            .map_err(|e| Error::external("get feed entity failed", e))?
            .map(|i| FeedEntity::new(i.data, self.fetch_cap.clone(), self.access_policy.clone())))
    }

    pub async fn delete(&self, entity: &FeedEntity) -> Result<(), Error> {
        self.repo
            .delete(entity.id())
            .await
            .map_err(|e| Error::external("feeds delete feed failed", e))
    }
}
