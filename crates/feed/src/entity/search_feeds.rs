use formatx::formatx;

use common::shared::{cap::FeedSearchUrlProvider, model::SearchUrls};

use crate::entity::model::FeedProp;

#[derive(Clone)]
pub struct SearchFeeds {
    data: Vec<FeedProp>,
}

impl SearchFeeds {
    pub(super) fn new(data: Vec<FeedProp>) -> Self {
        Self { data }
    }
}

impl FeedSearchUrlProvider for SearchFeeds {
    fn made_search_url(&self, keywords: &[String]) -> Vec<SearchUrls> {
        let mut res = vec![];
        for feed in &self.data {
            let mut urls = SearchUrls {
                feed_id: feed.data.id,
                urls: Vec::new(),
            };
            for key in keywords {
                if let Some(template) = &feed.data.metadata.search_url {
                    if let Ok(v) = formatx!(template, key) {
                        urls.urls.push(v);
                    } else {
                        tracing::error!(
                            "made_search_url formatx search url failed, template={template}, key={key}"
                        );
                    }
                }
            }
            res.push(urls);
        }
        res
    }
}
