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

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct DelRSS {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct RssItem {
    pub title: String,
    pub magnet: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct AnimeRssRecord {
    pub title: String,
    pub magnet: String,
    pub rule_name: String,
    pub info_hash: String,
}
