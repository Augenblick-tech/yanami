use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{
        AccessTokenClaims, AddEpsRequest, ApiResponse, BindRuleRequest, CreateSubscriptionRequest,
        DeleteEpsRequest, EpisodeItem, RecentEpisodeQuery, RecentEpisodeResponse,
        SearchStatusRequest,
    },
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::sync::Arc;
use subscription::entity::model::{EpsiodeStatus, MatchedEpisode};

/// 单次添加或删除的剧集条数上限，一次请求要读写的剧集行都在这个量级内
const MAX_EPS: usize = 100;

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
        // 订阅不再搜索：从搜索委托上去掉之后，读出来的状态才不是搜索中
        ctx.roots
            .search_mandates
            .remove_sub_anime(entity.id())
            .await?;
        entity.cancel_search();
    }

    ctx.roots.sub_animes.save(&entity).await?;

    Ok(Json(ApiResponse::ok(())))
}

/// 手动绑定规则
#[utoipa::path(
    post,
    path = "/api/v1/subscription/{id}/bind_rule",
    operation_id = "subscription_bind_rule",
    tag = "Subscription",
    summary = "手动绑定规则",
    description = "为指定番剧订阅手动绑定一个规则并清空历史剧集记录。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "订阅记录的唯一 ID")
    ),
    request_body = BindRuleRequest,
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
pub async fn bind_rule(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
    Json(req): Json<BindRuleRequest>,
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

    let Some(rule_entity) = ctx.roots.rules.find(req.rule_id).await? else {
        return Err(ApiError::not_found("not found rule"));
    };

    if rule_entity.space_id() != user_entity.space_id() {
        return Err(ApiError::forbidden("forbidden"));
    }

    let eps_collection = ctx.roots.sub_animes.as_eps(&entity).await;
    eps_collection.binding_rule(req.rule_id).await?;

    Ok(Json(ApiResponse::ok(())))
}

/// 重置整个番剧的下载状态
#[utoipa::path(
    put,
    path = "/api/v1/subscription/{id}/eps",
    operation_id = "subscription_reset_all_eps",
    tag = "Subscription",
    summary = "重置整个番剧的下载状态",
    description = "将指定番剧订阅下的所有剧集重置为未下载状态。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "番剧订阅记录 ID")
    ),
    responses(
        (status = 200, description = "重置成功"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "权限不足：该订阅记录不属于当前用户"),
        (status = 404, description = "找不到对应的订阅记录")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn reset_all_eps(
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

    let sub_anime_eps = ctx.roots.sub_animes.as_eps(&entity).await;
    let mut eps = sub_anime_eps.list().await?;
    for ep in &mut eps {
        ep.reset_download();
    }
    sub_anime_eps.save_epsiodes(&eps).await?;

    Ok(Json(ApiResponse::ok(())))
}

/// 重置单集剧集的下载状态
#[utoipa::path(
    put,
    path = "/api/v1/subscription/{id}/eps/{ep_id}",
    operation_id = "subscription_reset_ep",
    tag = "Subscription",
    summary = "重置单集剧集状态",
    description = "重置指定剧集的下载状态为未下载。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "番剧订阅记录 ID"),
        ("ep_id" = i64, Path, description = "剧集 ID")
    ),
    responses(
        (status = 200, description = "重置成功"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "权限不足：该订阅记录不属于当前用户"),
        (status = 404, description = "找不到对应的订阅记录或剧集")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_ep_status(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path((id, ep_id)): Path<(i64, i64)>,
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

    let sub_anime_eps = ctx.roots.sub_animes.as_eps(&entity).await;

    let Some(mut episode) = sub_anime_eps.get_epsiode(ep_id).await? else {
        return Err(ApiError::not_found("not found episode"));
    };

    episode.reset_download();
    sub_anime_eps.save_epsiode(&episode).await?;

    Ok(Json(ApiResponse::ok(())))
}

/// 添加匹配的剧集
#[utoipa::path(
    post,
    path = "/api/v1/subscription/{id}/eps",
    operation_id = "subscription_add_eps",
    tag = "Subscription",
    summary = "添加匹配的剧集",
    description = "把选中的资源作为指定订阅下的剧集落库。\n\n订阅已经绑了规则时不传 `rule_id` 就沿用，订阅还没有绑规则时必须传，且必须是同一个 space 下的规则，没有规则就报错。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "订阅记录的唯一 ID")
    ),
    request_body = AddEpsRequest,
    responses(
        (status = 200, description = "添加成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录或越权操作"),
        (status = 404, description = "资源不存在：未找到该订阅记录、规则记录或资源记录"),
        (status = 409, description = "冲突：订阅还没有绑定规则"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn add_eps(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
    Json(req): Json<AddEpsRequest>,
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

    // 规则只能是订阅所在空间下的，落库前先按这个口径读出来
    if let Some(rule_id) = req.rule_id {
        let Some(rule_entity) = ctx.roots.rules.find(rule_id).await? else {
            return Err(ApiError::not_found("not found rule"));
        };
        if rule_entity.space_id() != user_entity.space_id() {
            return Err(ApiError::forbidden("forbidden"));
        }
    }

    // 前端给的资源标识在这里校验：数量、是不是 hex、是不是 40 位，重复的只留一个
    if req.info_hashes.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            400,
            "info_hashes must not be empty",
        ));
    }
    if req.info_hashes.len() > MAX_EPS {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            400,
            format!(
                "info_hashes accepts at most {} items, got {}",
                MAX_EPS,
                req.info_hashes.len()
            ),
        ));
    }
    let mut info_hashes: Vec<[u8; 20]> = Vec::with_capacity(req.info_hashes.len());
    for info_hash in &req.info_hashes {
        let bytes = hex::decode(info_hash).map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                400,
                format!("info_hash {} is not a hex string", info_hash),
            )
        })?;
        let bytes: [u8; 20] = bytes.try_into().map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                400,
                format!("info_hash {} is not 40 hex characters", info_hash),
            )
        })?;
        if !info_hashes.contains(&bytes) {
            info_hashes.push(bytes);
        }
    }

    let resources = ctx.roots.resources.find_by_ids(&info_hashes).await?;
    if resources.len() != info_hashes.len() {
        return Err(ApiError::not_found("not found resource"));
    }

    // 资源是抓来的条目，落到订阅下就是剧集，这里只做这一层翻译
    let eps = resources
        .into_iter()
        .map(|resource| MatchedEpisode {
            sub_anime_id: entity.id(),
            resource_id: *resource.id(),
            status: EpsiodeStatus::Pending,
            title: resource.title().into(),
        })
        .collect::<Vec<_>>();

    ctx.roots
        .sub_animes
        .as_eps(&entity)
        .await
        .save_eps(req.rule_id, eps)
        .await?;

    Ok(Json(ApiResponse::ok(())))
}

/// 删除匹配的剧集
#[utoipa::path(
    delete,
    path = "/api/v1/subscription/{id}/eps",
    operation_id = "subscription_delete_eps",
    tag = "Subscription",
    summary = "删除匹配的剧集",
    description = "把指定 ID 的剧集从这条订阅里删掉，并按剩下的剧集重算订阅进度。剧集 ID 从该订阅的剧集列表里取。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("id" = i64, Path, description = "订阅记录的唯一 ID")
    ),
    request_body = DeleteEpsRequest,
    responses(
        (status = 200, description = "删除成功。返回数据的 `data` 字段为空。"),
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
pub async fn delete_eps(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Path(id): Path<i64>,
    Json(req): Json<DeleteEpsRequest>,
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

    // 剧集 ID 的取值在这里校验，属于哪条订阅由下面这条订阅的剧集来认
    if req.ep_ids.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            400,
            "ep_ids must not be empty",
        ));
    }
    if req.ep_ids.len() > MAX_EPS {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            400,
            format!(
                "ep_ids accepts at most {} items, got {}",
                MAX_EPS,
                req.ep_ids.len()
            ),
        ));
    }
    let mut ep_ids: Vec<i64> = Vec::with_capacity(req.ep_ids.len());
    for ep_id in &req.ep_ids {
        if *ep_id <= 0 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                400,
                format!("episode id {} is not a positive integer", ep_id),
            ));
        }
        if !ep_ids.contains(ep_id) {
            ep_ids.push(*ep_id);
        }
    }

    let sub_anime_eps = ctx.roots.sub_animes.as_eps(&entity).await;
    // 剧集 ID 只在这条订阅的剧集里认，别的订阅下的剧集不会因为 ID 相同而被删掉
    let eps = sub_anime_eps
        .list()
        .await?
        .into_iter()
        .filter(|i| ep_ids.contains(&i.id()))
        .collect::<Vec<_>>();
    sub_anime_eps.delete(&eps).await?;

    Ok(Json(ApiResponse::ok(())))
}

