use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path, State},
};

use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{AccessTokenClaims, ApiResponse, RuleCreateRequest, RuleItem, RuleUpdateOrderRequest},
};
use subscription::entity::model::RuleQuery;

/// 创建规则
#[utoipa::path(
    post,
    path = "/api/v1/rule",
    operation_id = "rule_add",
    tag = "Rule",
    summary = "创建规则",
    description = "创建一条新的规则。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = RuleCreateRequest,
    responses(
        (status = 200, description = "创建成功。返回创建的 `RuleItem` 对象。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn add(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<RuleCreateRequest>,
) -> Result<Json<ApiResponse<RuleItem>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let entity = ctx
        .roots
        .rules
        .create(&req.name, user_entity.space_id(), &req.pattern, req.order)
        .await?;
    Ok(Json(ApiResponse::ok(RuleItem::from(entity))))
}

/// 获取规则列表
#[utoipa::path(
    get,
    path = "/api/v1/rule",
    operation_id = "rule_list",
    tag = "Rule",
    summary = "获取规则列表",
    description = "获取所有的规则列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `[RuleItem]` 数组。"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
) -> Result<Json<ApiResponse<Vec<RuleItem>>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let query = RuleQuery {
        space_id: Some(user_entity.space_id()),
        active: Some(true),
    };
    let rules = ctx.roots.rules.list(&query).await?;
    Ok(Json(ApiResponse::ok(
        rules.into_iter().map(RuleItem::from).collect(),
    )))
}

/// 编辑规则优先级
#[utoipa::path(
    put,
    path = "/api/v1/rule/{rule_id}",
    operation_id = "rule_edit",
    tag = "Rule",
    summary = "修改规则优先级",
    description = "仅更新指定规则的优先级 (`order`) 信息。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("rule_id" = i64, Path, description = "需要更新的规则 ID")
    ),
    request_body = RuleUpdateOrderRequest,
    responses(
        (status = 200, description = "修改成功。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 404, description = "规则不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn edit(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(rule_id): Path<i64>,
    Json(req): Json<RuleUpdateOrderRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let Some(mut entity) = ctx.roots.rules.find(rule_id).await? else {
        return Err(ApiError::not_found("not found rule"));
    };

    if entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("not your rule"));
    }

    entity.set_order(req.order);

    ctx.roots.rules.save(&entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

/// 删除规则
#[utoipa::path(
    delete,
    path = "/api/v1/rule/{rule_id}",
    operation_id = "rule_delete",
    tag = "Rule",
    summary = "删除规则",
    description = "删除指定的规则。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("rule_id" = i64, Path, description = "需要删除的规则 ID")
    ),
    responses(
        (status = 200, description = "删除成功。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 404, description = "规则不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(rule_id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let Some(entity) = ctx.roots.rules.find(rule_id).await? else {
        return Err(ApiError::not_found("not found rule"));
    };

    if entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("not your rule"));
    }

    ctx.roots.rules.delete(&entity).await?;
    Ok(Json(ApiResponse::ok(())))
}
