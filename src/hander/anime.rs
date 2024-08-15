use anna::anime::anime::AnimeInfo;
use axum::{extract::Query, Extension, Json};

use crate::{
    common::{
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::{anime::AnimeRecordReq, rss::AnimeRssRecord},
    route::Service,
};

#[utoipa::path(
        get,
        path = "/v1/animes",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取所有番剧", body = JsonResultVecAnimeInfo)
        )
    )]
#[axum_macros::debug_handler]
pub async fn animes(
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<Vec<AnimeInfo>>>> {
    JsonResult::json_ok(service.anime_db.get_calender()?)
}

#[utoipa::path(
        get,
        path = "/v1/anime/records",
        params(
            AnimeRecordReq,
        ),
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取番剧下载记录", body = JsonResultVecAnimeRssRecord)
        )
    )]
#[axum_macros::debug_handler]
pub async fn anime_records(
    Extension(service): Extension<Service>,
    Query(q): Query<AnimeRecordReq>,
) -> ErrorResult<Json<JsonResult<Vec<AnimeRssRecord>>>> {
    if q.name.is_empty() {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(service.anime_db.get_anime_rss(q.name)?)
}
