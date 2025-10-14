use axum::{
    extract::{Path, Query},
    Extension, Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::route::Service;
use common::{
    errors::{Error, ErrorResult},
    result::JsonResult,
};
use model::{
    anime::{AnimeRecordReq, AnimeStatus, AnimesQuertOption, LatestAnimeRecordResponse},
    rss::AnimeRssRecord,
};

#[utoipa::path(
        get,
        path = "/v1/animes",
        params(
            AnimesQuertOption,
        ),
        security(("api_key" = ["Authorization"])),
        params(
            ("enable" = Option<bool>, Query, description = "是否启用"),
            ("search" = Option<bool>, Query, description = "是否启用搜索"),
            ("status" = Option<i64>, Query, description = "进度状态, 0: 进度为0, 1: 进度大于0且未满, 2: 进度已满"),
            ("name" = Option<String>, Query, description = "按名字模糊搜索"),
        ),
        responses(
            (status = 200, description = "获取番剧列表", body = JsonResultVecAnimeStatus)
        )
    )]
#[axum_macros::debug_handler]
pub async fn animes(
    Extension(service): Extension<Service>,
    Query(q): Query<AnimesQuertOption>,
) -> ErrorResult<Json<JsonResult<Vec<AnimeStatus>>>> {
    JsonResult::json_ok(Some(service.anime_db.get_calenders_with_query(Some(q)).await?))
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
    JsonResult::json_ok(service.anime_db.get_anime_rss_recodes(q.name_id).await?)
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
    service.anime_db.set_calender(req).await?;
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
    JsonResult::json_ok(service.anime_db.get_calender(id).await?)
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
    JsonResult::json_ok(service.anime_db.search_calender(name, None).await?)
}


#[derive(Deserialize, IntoParams, ToSchema)]
pub struct LatestAnimeQuery {
    n: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/animes/latest",
    params(
        LatestAnimeQuery
    ),
    security(("api_key" = ["Authorization"])),
    responses(
        (status = 200, description = "获取最近更新的剧集", body = JsonResultVecLatestAnimeRecordResponse)
    )
)]
#[axum_macros::debug_handler]
pub async fn latest_anime_records(
    Extension(service): Extension<Service>,
    Query(q): Query<LatestAnimeQuery>,
) -> ErrorResult<Json<JsonResult<Vec<LatestAnimeRecordResponse>>>> {
    let n = q.n.unwrap_or(10);
    let records = service.anime_db.latest_anime_records(n).await?;
    let mut response = Vec::new();
    for record in records {
        if let Some(anime) = service.anime_db.get_calender(record.anime_id).await? {
            response.push(LatestAnimeRecordResponse {
                record,
                anime,
            });
        }
    }
    JsonResult::json_ok(Some(response))
}
