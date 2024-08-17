use anna::anime::anime::AnimeInfo;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct AnimeRecordReq {
    pub name_id: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct AnimeStatus {
    pub status: bool,
    pub anime_info: AnimeInfo,
}
