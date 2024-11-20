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
    #[serde(default)]
    pub is_lock: bool,
    #[serde(default)]
    pub progress: usize,
}

impl From<entity::anime::Model> for AnimeStatus {
    fn from(value: entity::anime::Model) -> Self {
        Self {
            status: value.status,
            rule_name: value.rule_name,
            anime_info: serde_json::from_value(value.anime_info).unwrap(),
            is_search: value.is_search,
            is_lock: value.is_lock,
            progress: value.progress as usize,
        }
    }
}
