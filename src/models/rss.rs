use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RSSReq {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RSS {
    pub id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct DelRSS {
    pub id: String,
}
