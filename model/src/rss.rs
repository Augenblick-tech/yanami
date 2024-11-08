use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RSSReq {
    pub id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub search_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RSS {
    pub id: String,
    pub url: Option<String>,
    pub title: String,
    pub search_url: Option<String>,
}

impl From<entity::rss::Model> for RSS {
    fn from(value: entity::rss::Model) -> Self {
        Self {
            id: value.id,
            url: value.url,
            title: value.title,
            search_url: value.search_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct DelRSS {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct RssItem {
    pub title: String,
    pub magnet: String,
    pub pub_date: Option<String>,
    pub rule_name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct AnimeRssRecord {
    pub title: String,
    pub magnet: String,
    pub rule_name: String,
    pub info_hash: String,
}

impl From<entity::anime_record::Model> for AnimeRssRecord {
    fn from(value: entity::anime_record::Model) -> Self {
        Self {
            title: value.title,
            magnet: value.magnet,
            rule_name: value.rule_name,
            info_hash: value.info_hash,
        }
    }
}
