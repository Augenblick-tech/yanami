use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{ApiResponse, FeedItem, FeedItemRequest},
};

/// 创建Feed
#[utoipa::path(
    post,
    path = "/api/v1/feed",
    operation_id = "feed_add",
    tag = "Feed",
    summary = "创建 RSS 订阅源",
    description = "创建一个新的 RSS 订阅源（Feed）。\n\n- `title` 是必填字段，表示订阅源的名称。\n- `site_url` 和 `search_url` 不能同时为空，至少需要提供一个以便进行订阅更新或搜索。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = FeedItemRequest,
    responses(
        (status = 200, description = "创建成功。返回数据的 `data` 字段为创建成功的 `FeedItem` 对象。"),
        (status = 400, description = "请求参数校验失败：`site_url` 和 `search_url` 同时为空，或 JSON 格式错误"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：需要管理员权限"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn add(
    State(ctx): State<Arc<AppContext>>,
    Json(req): Json<FeedItemRequest>,
) -> Result<Json<ApiResponse<FeedItem>>, ApiError> {
    if req.site_url.is_none() && req.search_url.is_none() {
        return Err(ApiError::invalid_request());
    }
    let entity = ctx
        .roots
        .feeds
        .create(req.title, req.site_url, req.search_url)
        .await?;

    Ok(Json(ApiResponse::ok(FeedItem::from(entity))))
}

/// 获取Feed
#[utoipa::path(
    get,
    path = "/api/v1/feed",
    operation_id = "feed_list",
    tag = "Feed",
    summary = "获取 RSS 订阅源列表",
    description = "获取系统中所有的 RSS 订阅源列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `[FeedItem]` 数组，包含所有订阅源详情。"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list(
    State(ctx): State<Arc<AppContext>>,
) -> Result<Json<ApiResponse<Vec<FeedItem>>>, ApiError> {
    let feed_entity_list = ctx.roots.feeds.list().await?;
    Ok(Json(ApiResponse::ok(
        feed_entity_list.into_iter().map(FeedItem::from).collect(),
    )))
}

/// 删除Feed
#[utoipa::path(
    delete,
    path = "/api/v1/feed/{feed_id}",
    operation_id = "feed_delete",
    tag = "Feed",
    summary = "删除指定的 RSS 订阅源",
    description = "根据订阅源 ID (`feed_id`) 删除对应的 RSS 订阅源。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("feed_id" = i64, Path, description = "需要删除的订阅源的唯一 ID")
    ),
    responses(
        (status = 200, description = "删除成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：需要管理员权限"),
        (status = 404, description = "资源不存在：找不到指定的订阅源"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete(
    State(ctx): State<Arc<AppContext>>,
    Path(feed_id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(entity) = ctx.roots.feeds.get(feed_id).await? else {
        return Err(ApiError::not_found("not found feed"));
    };
    ctx.roots.feeds.delete(&entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

/// 编辑Feed
#[utoipa::path(
    put,
    path = "/api/v1/feed/{feed_id}",
    operation_id = "feed_edit",
    tag = "Feed",
    summary = "编辑/更新 RSS 订阅源",
    description = "根据订阅源 ID (`feed_id`) 更新该 RSS 订阅源的信息。\n\n- 可以更新名称、主页 URL 或搜索 URL。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("feed_id" = i64, Path, description = "需要更新的订阅源的唯一 ID")
    ),
    request_body = FeedItemRequest,
    responses(
        (status = 200, description = "编辑更新成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败或 JSON 格式错误"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：需要管理员权限"),
        (status = 404, description = "资源不存在：找不到指定的订阅源"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn edit(
    State(ctx): State<Arc<AppContext>>,
    Path(feed_id): Path<i64>,
    Json(req): Json<FeedItemRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut entity) = ctx.roots.feeds.get(feed_id).await? else {
        return Err(ApiError::not_found("not found feed"));
    };
    entity.set(req.title, req.site_url, req.search_url).await?;
    ctx.roots.feeds.save(&entity).await?;
    Ok(Json(ApiResponse::ok(())))
}
