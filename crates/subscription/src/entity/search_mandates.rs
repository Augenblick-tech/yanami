use std::sync::Arc;

use common::shared::{error::Error, model::SearchUrls};
use feed::entity::cap::{FeedAccessPolicy, FeedFetcher};

use crate::entity::{
    cap::SearchMandateRepository, model::Mandate, search_mandate_entity::SearchMandateEntity,
};

#[derive(Clone)]
pub struct SearchMandates {
    repo: Arc<dyn SearchMandateRepository>,
    fetch_cap: Arc<dyn FeedFetcher>,
    access_policy: Arc<dyn FeedAccessPolicy>,
}

impl SearchMandates {
    pub fn new(
        repo: Arc<dyn SearchMandateRepository>,
        fetch_cap: Arc<dyn FeedFetcher>,
        access_policy: Arc<dyn FeedAccessPolicy>,
    ) -> Self {
        Self {
            repo,
            fetch_cap,
            access_policy,
        }
    }
}

impl SearchMandates {
    // get_one
    // 任意获取一个搜索委托，允许传入过滤掉的feed_ids
    pub async fn get_one(&self) -> Result<Option<SearchMandateEntity>, Error> {
        let block_feed_ids = self.access_policy.block_feed_ids();
        let prop = self
            .repo
            .get_one(&block_feed_ids)
            .await
            .map_err(|e| Error::external("search manadate get one manadate failed", e))?;
        if let Some(prop) = prop {
            Ok(Some(SearchMandateEntity::new(
                prop.data,
                self.fetch_cap.clone(),
                self.access_policy.clone(),
            )))
        } else {
            Ok(None)
        }
    }

    // completed
    // 提交完成委托，并返回该委托是否为系列委托的最后一个
    pub async fn completed(&self, entity: SearchMandateEntity) -> Result<bool, Error> {
        if !entity.is_completed() {
            return Err(Error::conflict(format!(
                "search mandate {} is not completed",
                entity.id()
            )));
        }
        let count = self
            .repo
            .delete_and_count(entity.id(), entity.anime_id())
            .await
            .map_err(|e| Error::external("search manadate completed manadate failed", e))?;
        Ok(count == 0)
    }

    pub async fn create_from_search_urls(
        &self,
        anime_id: i64,
        urls: Vec<SearchUrls>,
    ) -> Result<Vec<SearchMandateEntity>, Error> {
        let mut mandates = vec![];
        for i in urls {
            let list = i
                .urls
                .into_iter()
                .map(|url| Mandate {
                    anime_id,
                    feed_id: i.feed_id,
                    url,
                })
                .collect::<Vec<_>>();
            mandates.extend(list);
        }
        self.create(&mandates).await
    }

    // create
    // 创建委托，委托存在时不报错
    pub async fn create(&self, mandates: &[Mandate]) -> Result<Vec<SearchMandateEntity>, Error> {
        let props = self
            .repo
            .save(mandates)
            .await
            .map_err(|e| Error::external("create search mandate failed", e))?;
        Ok(props
            .into_iter()
            .map(|prop| {
                SearchMandateEntity::new(
                    prop.data,
                    self.fetch_cap.clone(),
                    self.access_policy.clone(),
                )
            })
            .collect())
    }

    pub async fn drop(&self, entity: SearchMandateEntity) -> Result<bool, Error> {
        let count = self
            .repo
            .delete_and_count(entity.id(), entity.anime_id())
            .await
            .map_err(|e| Error::external("search mandates drop failed", e))?;
        Ok(count == 0)
    }
}
