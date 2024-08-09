use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RSSReq {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RSS {
    pub id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DelRSS {
    pub id: String,
}
