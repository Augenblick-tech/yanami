use axum::{extract::Query, Extension, Json};

use crate::{
    common::{
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::rss::{DelRSS, RSSReq, RSS},
    route::Service,
};

#[utoipa::path(
        get,
        path = "/v1/rss",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取所有RSS订阅", body = JsonResultVecRSS)
        )
    )]
#[axum_macros::debug_handler]
pub async fn rss_list(
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<Vec<RSS>>>> {
    JsonResult::json_ok(service.rss.get_all_rss().expect("get all rss failed"))
}

#[utoipa::path(
        post,
        path = "/v1/rss",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "添加RSS订阅", body = JsonResultRSS)
        )
    )]
#[axum_macros::debug_handler]
pub async fn set_rss(
    Extension(service): Extension<Service>,
    Json(req): Json<RSSReq>,
) -> ErrorResult<Json<JsonResult<RSS>>> {
    if req.url == "" {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(Some(service.rss.set_rss(req).expect("set rss failed")))
}

#[utoipa::path(
        delete,
        path = "/v1/rss",
        params(
            DelRSS,
        ),
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "删除RSS订阅", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn del_rss(
    Extension(service): Extension<Service>,
    Query(params): Query<DelRSS>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    if params.id == "" {
        return Err(Error::InvalidRequest);
    }
    service.rss.del_rss(params.id).expect("del rss failed");
    JsonResult::json_ok(None)
}
