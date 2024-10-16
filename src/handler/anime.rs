use axum::{
    extract::{Path, Query},
    Extension, Json,
};

use crate::{
    common::{
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::{
        anime::{AnimeRecordReq, AnimeStatus},
        rss::AnimeRssRecord,
    },
    route::Service,
};

#[utoipa::path(
        get,
        path = "/v1/animes",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取所有番剧", body = JsonResultVecAnimeStatus)
        )
    )]
#[axum_macros::debug_handler]
pub async fn animes(
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<Vec<AnimeStatus>>>> {
    JsonResult::json_ok(service.anime_db.get_calenders()?)
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
    if q.name_id <= 0 {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(service.anime_db.get_anime_rss_recodes(q.name_id)?)
}

#[utoipa::path(
        post,
        path = "/v1/anime",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "编辑番剧", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn set_anime(
    Extension(service): Extension<Service>,
    Json(req): Json<AnimeStatus>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    service.anime_db.set_calender(req)?;
    JsonResult::json_ok(None)
}

#[utoipa::path(
        get,
        path = "/v1/anime/{id}",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取番剧", body = JsonResultAnimeStatus)
        ),
        params(
            ("id" = i64, Path, description = "番剧id")
        )
    )]
#[axum_macros::debug_handler]
pub async fn get_anime(
    Extension(service): Extension<Service>,
    Path(id): Path<i64>,
) -> ErrorResult<Json<JsonResult<AnimeStatus>>> {
    JsonResult::json_ok(service.anime_db.get_calender(id)?)
}

#[utoipa::path(
        get,
        path = "/v1/anime/search/{name}",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "搜索番剧", body = JsonResultVecAnimeStatus)
        ),
        params(
            ("name" = String, Path, description = "番剧关键字")
        )
    )]
#[axum_macros::debug_handler]
pub async fn search_anime(
    Extension(service): Extension<Service>,
    Path(name): Path<String>,
) -> ErrorResult<Json<JsonResult<Vec<AnimeStatus>>>> {
    JsonResult::json_ok(service.anime_db.search_calender(name)?)
}
