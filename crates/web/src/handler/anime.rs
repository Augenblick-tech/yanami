use std::sync::Arc;

use crate::app_ctx::AppContext;
use crate::{
    error::ApiError,
    model::{
        AnimeMetadataItem, AnimeResponse, ApiResponse, CreateAnimeRequest, EditAnimeRequest, Page,
        PageAnimeRequest, SearchAnimeItem, SearchAnimeQuery,
    },
};
use anime::entity::model::AnimeMetadata;
use axum::{
    Json,
    extract::{Path, Query, State},
};

/// 获取番剧列表
#[utoipa::path(
    post,
    path = "/api/v1/anime",
    operation_id = "anime_list",
    tag = "Anime",
    summary = "获取番剧列表",
    description = "获取所有的番剧列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = PageAnimeRequest,
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `Page<Vec<AnimeResponse>>` 对象。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]

pub async fn list(
    State(ctx): State<Arc<AppContext>>,
    axum::Extension(user): axum::Extension<crate::model::AccessTokenClaims>,
    Json(request): Json<PageAnimeRequest>,
) -> Result<Json<ApiResponse<Page<Vec<AnimeResponse>>>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };
    let page = request.page.unwrap_or(1).max(1);
    let page_size = request.page_size.unwrap_or(10).max(1);
    let result = ctx
        .queries
        .anime_view
        .page_anime_views(&request, user_entity.space_id(), page, page_size)
        .await?;
    Ok(Json(ApiResponse::ok(result)))
}

/// 搜索番剧
#[utoipa::path(
    get,
    path = "/api/v1/anime/search",
    operation_id = "anime_search",
    tag = "Anime",
    summary = "搜索番剧",
    description = "通过关键字搜索对应的番剧。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("keyword" = String, Query, description = "搜索关键字")
    ),
    responses(
        (status = 200, description = "搜索成功。返回数据的 `data` 字段为 `[SearchAnimeItem]` 数组。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn search(
    State(ctx): State<Arc<AppContext>>,
    Query(query): Query<SearchAnimeQuery>,
) -> Result<Json<ApiResponse<Vec<SearchAnimeItem>>>, ApiError> {
    let result = ctx.roots.anime_source.search(&query.keyword).await?;
    let items = result.into_iter().map(SearchAnimeItem::from).collect();
    Ok(Json(ApiResponse::ok(items)))
}

/// 获取 Bangumi 番剧详细信息
#[utoipa::path(
    get,
    path = "/api/v1/anime/bgm/{bgm_id}",
    operation_id = "anime_bgm_info",
    tag = "Anime",
    summary = "获取 Bangumi 番剧详细信息",
    description = "根据 Bangumi ID 获取番剧的详细元数据。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("bgm_id" = i64, Path, description = "Bangumi 番剧 ID")
    ),
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `AnimeMetadataItem` 对象或 null。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn bgm_info(
    State(ctx): State<Arc<AppContext>>,
    Path(bgm_id): Path<i64>,
) -> Result<Json<ApiResponse<Option<AnimeMetadataItem>>>, ApiError> {
    let metadata = ctx.roots.anime_source.lookup_by_id(bgm_id).await?;
    let resp = metadata.map(AnimeMetadataItem::from);
    Ok(Json(ApiResponse::ok(resp)))
}

/// 添加番剧
#[utoipa::path(
    post,
    path = "/api/v1/anime/create",
    operation_id = "anime_create",
    tag = "Anime",
    summary = "添加番剧",
    description = "添加一个新的番剧到系统中。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = CreateAnimeRequest,
    responses(
        (status = 200, description = "添加成功。返回数据的 `data` 字段为新增番剧的 ID。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn create(
    State(ctx): State<Arc<AppContext>>,
    Json(request): Json<CreateAnimeRequest>,
) -> Result<Json<ApiResponse<i64>>, ApiError> {
    let metadata: AnimeMetadata = request.metadata.into();

    let mut entity = ctx.roots.animes.create(metadata).await?;
    if request.lock {
        entity.lock();
        ctx.roots.animes.save(&entity).await?;
    }

    Ok(Json(ApiResponse::ok(entity.id())))
}

/// 编辑番剧
#[utoipa::path(
    put,
    path = "/api/v1/anime/{anime_id}",
    operation_id = "anime_edit",
    tag = "Anime",
    summary = "编辑番剧",
    description = "手动编辑番剧的元数据。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("anime_id" = i64, Path, description = "番剧 ID")
    ),
    request_body = EditAnimeRequest,
    responses(
        (status = 200, description = "编辑成功"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 404, description = "未找到该番剧"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn edit(
    State(ctx): State<Arc<AppContext>>,
    Path(anime_id): Path<i64>,
    Json(request): Json<EditAnimeRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let mut entity = ctx
        .roots
        .animes
        .get(anime_id)
        .await?
        .ok_or_else(|| ApiError::not_found("anime not found"))?;

    let metadata: AnimeMetadata = request.metadata.into();

    entity.force_update_metadata(&metadata);

    if let Some(lock) = request.lock {
        if lock {
            entity.lock();
        } else {
            entity.unlock();
        }
    }

    ctx.roots.animes.save(&entity).await?;

    Ok(Json(ApiResponse::ok(())))
}
