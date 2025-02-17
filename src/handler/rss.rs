use anyhow::Context;
use axum::{extract::Query, Extension, Json};
use formatx::formatx;

use crate::route::Service;
use common::{
    errors::{Error, ErrorResult},
    result::JsonResult,
};
use model::rss::{DelRSS, RSSReq, RSS};

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
    JsonResult::json_ok(
        service
            .rss_db
            .get_all_rss()
            .await
            .context("get all rss failed")?,
    )
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
    Json(mut req): Json<RSSReq>,
) -> ErrorResult<Json<JsonResult<RSS>>> {
    if req.url.is_none() && req.search_url.is_none() {
        return Err(Error::InvalidRequest);
    }

    if req.title.is_none() {
        if let Some(url) = &req.url {
            let chan = service
                .rss_http_client
                .get_channel(url)
                .await
                .context("get rss channel failed")?;
            req.title = Some(chan.title);
        } else if let Some(search_url) = &req.search_url {
            let chan = service
                .rss_http_client
                .get_channel(
                    &formatx!(search_url, "test").context("create test search url failed")?,
                )
                .await
                .context("get rss search_url channel failed")?;
            req.title = Some(chan.title);
        }
    }

    if req.title.is_none() {
        return Err(Error::InvalidRequest);
    }

    JsonResult::json_ok(Some(
        service
            .rss_db
            .set_rss(req)
            .await
            .map_err(|e| anyhow::Error::msg(format!("set rss failed, {}", e)))?,
    ))
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
    if params.id.is_empty() {
        return Err(Error::InvalidRequest);
    }
    service
        .rss_db
        .del_rss(params.id)
        .await
        .context("del rss failed")?;
    JsonResult::json_ok(None)
}
