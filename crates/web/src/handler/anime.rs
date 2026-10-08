use std::sync::Arc;

use crate::app_ctx::AppContext;
use crate::{
    error::ApiError,
    model::{
        AnimeMetadataItem, AnimeResponse, ApiResponse, CreateAnimeRequest, EditAnimeRequest, Page,
        PageAnimeRequest, SearchAnimeItem, SearchAnimeQuery,
    },
};
use anime::entity::model::{AnimeIdType, AnimeMetadata, AnimeSourceTarget};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
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
    require_series_metadata(&metadata)?;

    let entity = ctx.roots.animes.create(metadata, request.lock).await?;

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
    require_series_metadata(&metadata)?;

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

/// 取元数据里的 TMDB 系列 id：不是正数（含非数字字符串）都视为该番剧没有系列身份。
fn tmdb_series_id(metadata: &AnimeMetadata) -> Option<i64> {
    metadata
        .external_link
        .iter()
        .filter(|link| link.target == AnimeSourceTarget::TMDB)
        .find_map(|link| match &link.id {
            AnimeIdType::Int(id) if *id > 0 => Some(*id),
            AnimeIdType::String(id) => id.parse::<i64>().ok().filter(|id| *id > 0),
            _ => None,
        })
}

/// 具备 TMDB 系列身份的番剧必须带回系列信息：系列信息决定系列名与落点季号，
/// 缺了它系列信息不完整，下载时解析不出落点，因此非法请求在处理前就拦下来。
fn require_series_metadata(metadata: &AnimeMetadata) -> Result<(), ApiError> {
    let Some(tmdb_id) = tmdb_series_id(metadata) else {
        return Ok(());
    };

    if metadata.series_metadata.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            400,
            format!("series metadata is required for anime with tmdb id {tmdb_id}"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime::entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeLangTarget, AnimeSeriesMetadata, AnimeTitle,
    };
    use chrono::NaiveDate;

    fn metadata(link: AnimeEx, series: Option<AnimeSeriesMetadata>) -> AnimeMetadata {
        AnimeMetadata {
            series_metadata: series,
            external_link: vec![link],
            titles: vec![AnimeTitle {
                name: "転生したら剣でした".to_string(),
                match_name: "転生したら剣でした".to_string(),
                target: AnimeLangTarget::JP,
                origin: true,
            }],
            air_weekday: Some(AnimeAirWeekday::Wednesday),
            air_date: NaiveDate::from_ymd_opt(2022, 10, 5).unwrap(),
            air_quarter: 202210,
            season: Vec::new(),
        }
    }

    fn tmdb_link(id: AnimeIdType) -> AnimeEx {
        AnimeEx {
            id,
            target: AnimeSourceTarget::TMDB,
            r#type: Some("tv".to_string()),
        }
    }

    fn series() -> AnimeSeriesMetadata {
        AnimeSeriesMetadata {
            origin_name: "転生したら剣でした".to_string(),
            cn_name: "转生就是剑".to_string(),
            desc: String::new(),
            air_date: NaiveDate::from_ymd_opt(2022, 10, 5).unwrap(),
            genres: Vec::new(),
        }
    }

    #[test]
    fn require_series_metadata_rejects_tmdb_anime_without_series() {
        let metadata = metadata(tmdb_link(AnimeIdType::Int(65942)), None);

        let error = require_series_metadata(&metadata)
            .expect_err("tmdb anime without series metadata must be rejected");
        let debug = format!("{error:?}");
        assert!(
            debug.contains("400"),
            "should be a 400 error, actual {debug}"
        );
        assert!(
            debug.contains("series metadata is required for anime with tmdb id 65942"),
            "error should name the tmdb id, actual {debug}"
        );
    }

    #[test]
    fn require_series_metadata_rejects_string_tmdb_id_without_series() {
        let metadata = metadata(tmdb_link(AnimeIdType::String("330431".to_string())), None);

        assert!(require_series_metadata(&metadata).is_err());
    }

    #[test]
    fn require_series_metadata_accepts_tmdb_anime_with_series() {
        let metadata = metadata(tmdb_link(AnimeIdType::Int(65942)), Some(series()));

        assert!(require_series_metadata(&metadata).is_ok());
    }

    #[test]
    fn require_series_metadata_accepts_anime_without_tmdb_identity() {
        let metadata = metadata(
            AnimeEx {
                id: AnimeIdType::Int(425998),
                target: AnimeSourceTarget::Bangumi,
                r#type: Some("TV".to_string()),
            },
            None,
        );

        assert!(require_series_metadata(&metadata).is_ok());
    }

    #[test]
    fn require_series_metadata_accepts_zero_tmdb_id() {
        let metadata = metadata(tmdb_link(AnimeIdType::Int(0)), None);

        assert!(require_series_metadata(&metadata).is_ok());
    }
}
