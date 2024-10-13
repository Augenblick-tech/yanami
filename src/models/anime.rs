use anna::anime::tracker::AnimeInfo;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct AnimeRecordReq {
    pub name_id: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams, Clone)]
pub struct AnimeStatus {
    pub status: bool,
    pub rule_name: String,
    pub anime_info: AnimeInfo,
    #[serde(default)]
    pub is_search: bool,
}
