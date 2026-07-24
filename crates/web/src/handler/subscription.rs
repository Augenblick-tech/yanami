use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{
        AccessTokenClaims, ApiResponse, CreateSubscriptionRequest, EpisodeItem, RecentEpisodeQuery,
        RecentEpisodeResponse, SearchStatusRequest,
    },
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use std::sync::Arc;

/// 创建订阅
#[utoipa::path(
    post,
    path = "/api/v1/subscription",
    operation_id = "subscription_add",
    tag = "Subscription",
    summary = "创建番剧订阅",
    description = "创建一个新的番剧订阅。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = CreateSubscriptionRequest,
    responses(
        (status = 200, description = "创建成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：找不到该对应的用户记录"),
        (status = 404, description = "未找到：番剧不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn add(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    if ctx.roots.animes.get(req.anime_id).await?.is_none() {
        return Err(ApiError::not_found("not found anime"));
    }

    ctx.roots
        .sub_animes
        .create(user_entity.space_id(), req.anime_id)
        .await?;
    Ok(Json(ApiResponse::ok(())))
}

/// 取消订阅
#[utoipa::path(
    delete,
    path = "/api/v1/subscription/{id}",
    operation_id = "subscription_delete",
    tag = "Subscription",
    summary = "取消番剧订阅",
    description = "取消指定的番剧订阅。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "需要取消的订阅记录的唯一 ID")
    ),
    responses(
        (status = 200, description = "取消成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录或越权操作"),
        (status = 404, description = "资源不存在：未找到该订阅记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let Some(entity) = ctx.roots.sub_animes.find_by_sub_anime_id(id).await? else {
        return Err(ApiError::not_found("not found subscription"));
    };

    if entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("forbidden"));
    }

    ctx.roots.sub_animes.unsub(&entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

/// 获取订阅剧集列表
#[utoipa::path(
    get,
    path = "/api/v1/subscription/{id}/episode",
    operation_id = "subscription_list_eps",
    tag = "Subscription",
    summary = "获取订阅剧集列表",
    description = "获取指定番剧订阅的所有剧集列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "订阅记录的唯一 ID")
    ),
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `[EpisodeItem]` 数组，包含该订阅下的所有剧集信息。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录或越权操作"),
        (status = 404, description = "资源不存在：未找到该订阅记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_eps(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<Vec<EpisodeItem>>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let Some(sub_anime_entity) = ctx.roots.sub_animes.find_by_sub_anime_id(id).await? else {
        return Err(ApiError::not_found("not found subscription"));
    };

    if sub_anime_entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("forbidden"));
    }

    let eps_collection = ctx.roots.sub_animes.as_eps(&sub_anime_entity).await;
    let eps = eps_collection.list().await?;

    let items = eps.into_iter().map(EpisodeItem::from).collect();
    Ok(Json(ApiResponse::ok(items)))
}

/// 获取最近更新的剧集
#[utoipa::path(
    get,
    path = "/api/v1/subscription/recent",
    operation_id = "subscription_recent_episodes",
    tag = "Subscription",
    summary = "获取最近更新的剧集",
    description = "获取当前用户最近更新的10个剧集。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("lang" = Option<String>, Query, description = "目标语言名称")
    ),
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为剧集数组。"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn recent_episodes(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Query(query): Query<RecentEpisodeQuery>,
) -> Result<Json<ApiResponse<Vec<RecentEpisodeResponse>>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let result = ctx
        .queries
        .anime_view
        .recent_episodes(user_entity.space_id(), query.lang)
        .await?;

    Ok(Json(ApiResponse::ok(result)))
}

/// 启用或取消订阅搜索补全
#[utoipa::path(
    post,
    path = "/api/v1/subscription/{id}/search_status",
    operation_id = "subscription_set_search_status",
    tag = "Subscription",
    summary = "设置订阅搜索状态",
    description = "启用或取消指定订阅的搜索补全功能。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "订阅记录的唯一 ID")
    ),
    request_body = SearchStatusRequest,
    responses(
        (status = 200, description = "操作成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录或越权操作"),
        (status = 404, description = "资源不存在：未找到该订阅记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn set_search_status(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
    Json(req): Json<SearchStatusRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let Some(mut entity) = ctx.roots.sub_animes.find_by_sub_anime_id(id).await? else {
        return Err(ApiError::not_found("not found subscription"));
    };

    if entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("forbidden"));
    }

    if req.enable {
        entity.enable_search();
    } else {
        entity.cancel_search();
    }

    ctx.roots.sub_animes.save(&entity).await?;

    Ok(Json(ApiResponse::ok(())))
}
