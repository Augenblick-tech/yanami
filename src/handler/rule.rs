use axum::{extract::Query, Extension, Json};

use crate::route::Service;
use common::{
    errors::{Error, ErrorResult},
    result::JsonResult,
};
use model::rule::{DelRule, Rule};

#[utoipa::path(
        post,
        path = "/v1/rule",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "添加番剧匹配规则", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn set_rule(
    Extension(service): Extension<Service>,
    Json(req): Json<Rule>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    if req.name.is_empty() || req.re.is_empty() {
        return Err(Error::InvalidRequest);
    }

    service.rule_db.set_rule(req).await?;

    JsonResult::json_ok(None)
}

#[utoipa::path(
        get,
        path = "/v1/rules",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取所有规则", body = JsonResultVecRule)
        )
    )]
#[axum_macros::debug_handler]
pub async fn rules(
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<Vec<Rule>>>> {
    JsonResult::json_ok(service.rule_db.get_all_rules().await?)
}

#[utoipa::path(
        delete,
        path = "/v1/rule",
        params(
            DelRule,
        ),
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "删除规则", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn del_rule(
    Extension(service): Extension<Service>,
    Query(params): Query<DelRule>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    if params.name.is_empty() {
        return Err(Error::InvalidRequest);
    }
    service
        .rule_db
        .del_rule(params.name)
        .await
        .map_err(|e| anyhow::Error::msg(format!("del rule failed, {}", e)))?;
    JsonResult::json_ok(None)
}
