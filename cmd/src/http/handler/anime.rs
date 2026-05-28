use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header::ACCEPT_LANGUAGE, HeaderMap, StatusCode},
    Json,
};

use crate::http::{auth::AuthenticatedUser, error::ApiError, model::*, state::AppState};
use domain::anime::{
    AirDate, AnimeId, AnimeMetadata, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount,
    SeasonNumber,
};
use service::anime::service::AnimeCollectionFilter;

/// 查询番剧目录。
#[utoipa::path(
    get,
    path = "/api/v1/animes",
    security(("bearer_auth" = [])),
    params(AnimeQuery),
    responses((status = 200, description = "番剧目录查询成功。"))
)]
pub async fn list_animes(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Query(query): Query<AnimeQuery>,
) -> Result<Json<ApiResponse<PaginatedAnimeResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);
    let language = query.language.clone();
    let filter = AnimeCollectionFilter {
        enabled: query.enabled,
        search_enabled: query.search_enabled,
        subscribed: query.subscribed,
        metadata_locked: query.metadata_locked,
        progress_state: query.progress_state,
        keyword: query.keyword,
        year: query.year,
        month: query.month.map(|m| ((m - 1) / 3) * 3 + 1),
        page,
        page_size,
    };
    let outcome = state.anime_service.list_animes(space_id, filter).await?;
    Ok(Json(ApiResponse::ok(PaginatedAnimeResponse {
        items: outcome
            .items
            .into_iter()
            .map(|item| {
                AnimeViewResponse::from_item(
                    item,
                    language_preference(language.as_deref(), &headers),
                )
            })
            .collect(),
        total: outcome.total,
        page,
        page_size,
    })))
}

/// 查询单个番剧。
#[utoipa::path(
    get,
    path = "/api/v1/animes/{anime_id}",
    security(("bearer_auth" = [])),
    params(AnimeIdParam, AnimeLanguageQuery),
    responses(
        (status = 200, description = "查询成功。"),
    )
)]
pub async fn get_anime(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
    headers: HeaderMap,
    Query(query): Query<AnimeLanguageQuery>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime = state
        .anime_service
        .get_anime_item(user.user_id, space_id, AnimeId(path.anime_id))
        .await?;
    match anime {
        Some(item) => Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
            item,
            language_preference(query.language.as_deref(), &headers),
        )))),
        None => Err(ApiError::not_found("anime")),
    }
}

/// 查询最近更新。
#[utoipa::path(
    get,
    path = "/api/v1/animes/latest",
    security(("bearer_auth" = [])),
    params(LatestAnimeParam),
    responses((status = 200, description = "查询成功。"))
)]
pub async fn list_latest_anime_releases(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Query(query): Query<LatestAnimeParam>,
) -> Result<Json<ApiResponse<Vec<LatestAnimeViewResponse>>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let limit = query.limit.unwrap_or(10).min(20);
    let language = query.language.clone();
    let outcomes = state.anime_service.list_latest(space_id, limit).await?;
    Ok(Json(ApiResponse::ok(
        outcomes
            .into_iter()
            .map(|item| {
                LatestAnimeViewResponse::from_view(
                    item,
                    language_preference(language.as_deref(), &headers),
                )
            })
            .collect(),
    )))
}

/// 查询番剧仪表盘统计。
///
/// 返回全局及按季度的番剧状态分布（总数/已完结/更新中/未开始/已暂停/已订阅），
/// 以及搜索池中正在搜索的番剧数和待处理链接数。
#[utoipa::path(
    get,
    path = "/api/v1/animes/dashboard",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "返回番剧仪表盘统计和搜索池状态。"))
)]
pub async fn get_anime_dashboard(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<ApiResponse<AnimeDashboardResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let dashboard = state.anime_service.get_dashboard(space_id).await?;
    let search = build_search_stats(&state).await?;
    Ok(Json(ApiResponse::ok(AnimeDashboardResponse::from_view(
        dashboard, search,
    ))))
}

/// 从订阅上下文查询搜索池统计信息。
///
/// 返回正在搜索的番剧数（`search_pool` 表中 distinct anime_id 数）
/// 和剩余待处理链接数（`search_pool` 表总行数）。
async fn build_search_stats(
    state: &AppState,
) -> Result<AnimeDashboardSearchResponse, crate::http::error::ApiError> {
    let (searching_anime_count, pending_link_count) =
        state.subscription_service.get_search_pool_stats().await?;
    Ok(AnimeDashboardSearchResponse {
        searching_anime_count,
        pending_link_count,
    })
}

/// 查询番剧更新记录。
#[utoipa::path(
    get,
    path = "/api/v1/animes/{anime_id}/records",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    responses((status = 200, description = "查询成功。"))
)]
pub async fn list_anime_release_records(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
) -> Result<Json<ApiResponse<AnimeReleaseRecordsResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let outcome = state
        .anime_service
        .list_release_records(user.user_id, space_id, AnimeId(path.anime_id))
        .await?;
    Ok(Json(ApiResponse::ok(AnimeReleaseRecordsResponse {
        anime_id: outcome.anime_id.0,
        records: outcome.records.into_iter().map(Into::into).collect(),
    })))
}

/// 通过 Bangumi ID 预览番剧信息。
///
/// 系统会根据 bgm_id 从 Bangumi 获取番剧基本信息，再通过 TMDB 搜索匹配获取中文标题和别名。
/// 返回的数据可直接用于 `POST /api/v1/animes` 创建番剧。
#[utoipa::path(
    get,
    path = "/api/v1/animes/preview",
    security(("bearer_auth" = [])),
    params(BgmIdQuery),
    responses(
        (status = 200, description = "预览成功。返回番剧元数据，前端可展示并允许用户修改。", body = ApiResponse<CreateAnimeRequest>),
        (status = 400, description = "bgm_id 无效或 Bangumi 未找到对应番剧。", body = ErrorResponse),
        (status = 502, description = "Bangumi 或 TMDB 上游服务请求失败。", body = ErrorResponse),
    ),
)]
pub async fn preview_anime(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BgmIdQuery>,
) -> Result<Json<ApiResponse<CreateAnimeRequest>>, ApiError> {
    let bgm_id = AnimeId(query.bgm_id);
    let metadata = state
        .anime_service
        .preview_anime(&*state.single_anime_source, bgm_id)
        .await?;

    let req = CreateAnimeRequest {
        bgm_id: metadata.id.0,
        original_ja: metadata.titles.original_ja,
        localized_zh_cn: metadata.titles.localized_zh_cn,
        localized_zh_tw: metadata.titles.localized_zh_tw,
        search_name: metadata.titles.search_name,
        aliases: metadata.titles.aliases,
        broadcast_weekday: metadata.broadcast_weekday.0,
        planned_episode_count: metadata.planned_episode_count.0,
        air_date: metadata.air_date.0,
        season: metadata.season.0,
    };
    Ok(Json(ApiResponse::ok(req)))
}

/// 创建番剧并自动订阅。
///
/// 前端应先通过 `GET /api/v1/animes/preview?bgm_id=` 预览匹配结果，
/// 用户确认/修改后，将完整元数据提交至此接口。
/// 创建成功后，会为所有开启了自动订阅的空间自动创建订阅。
#[utoipa::path(
    post,
    path = "/api/v1/animes",
    security(("bearer_auth" = [])),
    params(AnimeLanguageQuery),
    request_body = CreateAnimeRequest,
    responses(
        (status = 201, description = "番剧创建成功。返回完整的番剧视图，包含订阅状态。", body = ApiResponse<AnimeViewResponse>),
        (status = 400, description = "请求字段校验失败。", body = ErrorResponse),
        (status = 409, description = "该 bgm_id 对应的番剧已存在。", body = ErrorResponse),
    ),
)]
pub async fn create_anime(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Query(query): Query<AnimeLanguageQuery>,
    Json(request): Json<CreateAnimeRequest>,
) -> Result<(StatusCode, Json<ApiResponse<AnimeViewResponse>>), ApiError> {
    let metadata = AnimeMetadata {
        id: AnimeId(request.bgm_id),
        titles: AnimeTitleSet {
            original_ja: request.original_ja,
            localized_zh_cn: request.localized_zh_cn,
            localized_zh_tw: request.localized_zh_tw,
            search_name: request.search_name,
            aliases: request.aliases,
        },
        broadcast_weekday: BroadcastWeekday(request.broadcast_weekday),
        planned_episode_count: PlannedEpisodeCount(request.planned_episode_count),
        air_date: AirDate(request.air_date),
        season: SeasonNumber(request.season),
    };
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = state.anime_service.create_anime(metadata).await?;
    if let Err(error) = state
        .subscription_service
        .auto_subscribe_new_animes(&[anime_id])
        .await
    {
        tracing::error!(?error, anime_id = %anime_id.0, "auto subscribe failed for newly created anime");
    }
    let item = state
        .anime_service
        .get_anime_item(user.user_id, space_id, anime_id)
        .await?
        .ok_or(ApiError::not_found("anime"))?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(AnimeViewResponse::from_item(
            item,
            language_preference(query.language.as_deref(), &headers),
        ))),
    ))
}

/// 更新番剧的全部元数据字段。
///
/// 此接口用于用户手动修正番剧信息，包括标题、别名、放送日期、集数等。
/// 即使番剧已被锁定（metadata_locked = true），用户仍可通过此接口修改元数据，
/// 锁定仅阻止上游定时同步覆盖。
/// 番剧 ID（bgm_id）不可修改，如需修改请删除后重新创建。
#[utoipa::path(
    put,
    path = "/api/v1/animes/{anime_id}/metadata",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    request_body = UpdateAnimeMetadataRequest,
    responses(
        (status = 200, description = "元数据更新成功。返回更新后的番剧视图。", body = ApiResponse<AnimeViewResponse>),
        (status = 400, description = "请求字段校验失败（例如 air_date 格式不是 yyyy-mm-dd、planned_episode_count 或 season 不大于 0）。", body = ErrorResponse),
        (status = 404, description = "指定 anime_id 的番剧不存在。", body = ErrorResponse),
    ),
)]
pub async fn update_anime_metadata(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
    Json(request): Json<UpdateAnimeMetadataRequest>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = AnimeId(path.anime_id);
    let metadata = AnimeMetadata {
        id: anime_id,
        titles: AnimeTitleSet {
            original_ja: request.original_ja,
            localized_zh_cn: request.localized_zh_cn,
            localized_zh_tw: request.localized_zh_tw,
            search_name: request.search_name,
            aliases: request.aliases,
        },
        broadcast_weekday: BroadcastWeekday(request.broadcast_weekday),
        planned_episode_count: PlannedEpisodeCount(request.planned_episode_count),
        air_date: AirDate(request.air_date),
        season: SeasonNumber(request.season),
    };
    let outcome = state
        .anime_service
        .update_anime_metadata(user.user_id, space_id, anime_id, metadata)
        .await?;
    Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
        outcome.item,
        DisplayLanguagePreference::default(),
    ))))
}

/// 获取番剧元数据，用于编辑表单预填。
///
/// 返回的字段结构与 `PUT /api/v1/animes/{anime_id}/metadata` 的请求体一致，
/// 前端可以直接填入编辑表单供用户修改。
#[utoipa::path(
    get,
    path = "/api/v1/animes/{anime_id}/metadata",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    responses(
        (status = 200, description = "返回番剧元数据，包含全部可编辑字段。", body = ApiResponse<UpdateAnimeMetadataRequest>),
        (status = 404, description = "指定 anime_id 的番剧不存在。", body = ErrorResponse),
    ),
)]
pub async fn get_anime_metadata(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
) -> Result<Json<ApiResponse<UpdateAnimeMetadataRequest>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = AnimeId(path.anime_id);
    let item = state
        .anime_service
        .get_anime_item(user.user_id, space_id, anime_id)
        .await?
        .ok_or(ApiError::not_found("anime"))?;
    let m = &item.anime.metadata;
    Ok(Json(ApiResponse::ok(UpdateAnimeMetadataRequest {
        original_ja: m.titles.original_ja.clone(),
        localized_zh_cn: m.titles.localized_zh_cn.clone(),
        localized_zh_tw: m.titles.localized_zh_tw.clone(),
        search_name: m.titles.search_name.clone(),
        aliases: m.titles.aliases.clone(),
        broadcast_weekday: m.broadcast_weekday.0,
        planned_episode_count: m.planned_episode_count.0,
        air_date: m.air_date.0.clone(),
        season: m.season.0,
    })))
}

/// 订阅番剧。
///
/// 订阅成功后返回的 `subscribed` 为 `true`。如果已订阅则直接返回当前状态。
#[utoipa::path(
    post,
    path = "/api/v1/animes/{anime_id}/subscription",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    responses(
        (status = 200, description = "订阅成功。已订阅时直接返回当前状态。", body = ApiResponse<AnimeViewResponse>),
        (status = 404, description = "番剧不存在。", body = ErrorResponse),
    ),
)]
pub async fn subscribe(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = AnimeId(path.anime_id);
    state
        .anime_service
        .subscribe(user.user_id, space_id, anime_id)
        .await?;
    let item = state
        .anime_service
        .get_anime_item(user.user_id, space_id, anime_id)
        .await?
        .ok_or(ApiError::not_found("anime"))?;
    Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
        item,
        DisplayLanguagePreference::default(),
    ))))
}

/// 取消订阅番剧。
///
/// 取消后返回的 `subscribed` 为 `false`。如果已取消则直接返回当前状态。
#[utoipa::path(
    delete,
    path = "/api/v1/animes/{anime_id}/subscription",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    responses(
        (status = 200, description = "取消订阅成功。已取消时直接返回当前状态。", body = ApiResponse<AnimeViewResponse>),
        (status = 404, description = "番剧不存在。", body = ErrorResponse),
    ),
)]
pub async fn unsubscribe(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = AnimeId(path.anime_id);
    state
        .anime_service
        .unsubscribe(user.user_id, space_id, anime_id)
        .await?;
    let item = state
        .anime_service
        .get_anime_item(user.user_id, space_id, anime_id)
        .await?
        .ok_or(ApiError::not_found("anime"))?;
    Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
        item,
        DisplayLanguagePreference::default(),
    ))))
}

/// 设置番剧订阅的活跃状态。
///
/// - `enabled: true` → 启用追更
/// - `enabled: false` → 暂停追更
///
/// 需要先通过 `POST` 订阅番剧，未订阅时返回 404。
/// 此接口仅控制活跃状态，不会创建或删除订阅记录。
#[utoipa::path(
    put,
    path = "/api/v1/animes/{anime_id}/subscription/active",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    request_body = SetSubscriptionRequest,
    responses(
        (status = 200, description = "活跃状态更新成功。", body = ApiResponse<AnimeViewResponse>),
        (status = 404, description = "番剧未订阅。", body = ErrorResponse),
    ),
)]
pub async fn set_subscription_active(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
    Json(request): Json<SetSubscriptionRequest>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let anime_id = AnimeId(path.anime_id);
    state
        .anime_service
        .set_active(user.user_id, space_id, anime_id, request.enabled)
        .await?;
    let item = state
        .anime_service
        .get_anime_item(user.user_id, space_id, anime_id)
        .await?
        .ok_or(ApiError::not_found("anime"))?;
    Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
        item,
        DisplayLanguagePreference::default(),
    ))))
}

/// 更新番剧属性。
///
/// 支持修改 `search_enabled`（主动搜索补全）和 `metadata_locked`（元数据锁定）两个字段。
#[utoipa::path(
    post,
    path = "/api/v1/animes/{anime_id}",
    security(("bearer_auth" = [])),
    params(AnimeIdParam),
    request_body = PatchAnimeRequest,
    responses((status = 200, description = "更新成功。"))
)]
pub async fn update_anime(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(path): Path<AnimeIdParam>,
    Json(request): Json<PatchAnimeRequest>,
) -> Result<Json<ApiResponse<AnimeViewResponse>>, ApiError> {
    let space_id = state
        .space_service
        .resolve_personal_space(user.user_id)
        .await?;
    let outcome = state
        .anime_service
        .patch_anime_item(
            user.user_id,
            space_id,
            AnimeId(path.anime_id),
            request.search_enabled,
            request.metadata_locked,
        )
        .await?;
    Ok(Json(ApiResponse::ok(AnimeViewResponse::from_item(
        outcome.item,
        DisplayLanguagePreference::default(),
    ))))
}

fn language_preference<'a>(
    query_language: Option<&'a str>,
    headers: &'a HeaderMap,
) -> DisplayLanguagePreference<'a> {
    DisplayLanguagePreference {
        query_language,
        accept_language: headers
            .get(ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok()),
    }
}
