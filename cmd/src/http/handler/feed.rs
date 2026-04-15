use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::http::{error::ApiError, model::*, state::AppState};
use domain::feed::{FeedSource, FeedSourceId};

/// 查询空间下的 RSS 来源。
#[utoipa::path(
    get,
    path = "/api/v1/space/feeds",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "RSS 来源查询成功。"))
)]
pub async fn get_feeds(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<FeedsResponse>>, ApiError> {
    let outcome = state.feed_service.get_feeds(state.admin_space_id).await?;
    Ok(Json(ApiResponse::ok(FeedsResponse {
        sources: outcome.sources.into_iter().map(Into::into).collect(),
    })))
}

/// 新增空间 RSS 来源。
#[utoipa::path(
    post,
    path = "/api/v1/space/feeds",
    security(("bearer_auth" = [])),
    request_body = SaveFeedSourceRequest,
    responses((status = 200, description = "新增成功。"))
)]
pub async fn create_feed(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SaveFeedSourceRequest>,
) -> Result<Json<ApiResponse<FeedSourceView>>, ApiError> {
    save_single_feed(state, request.into_domain(String::new())).await
}

/// 修改单个空间 RSS 来源。
#[utoipa::path(
    put,
    path = "/api/v1/space/feeds/{feed_id}",
    security(("bearer_auth" = [])),
    request_body = SaveFeedSourceRequest,
    params(("feed_id" = String, Path, description = "RSS 来源标识")),
    responses((status = 200, description = "修改成功。"))
)]
pub async fn update_feed(
    State(state): State<Arc<AppState>>,
    Path(feed_id): Path<String>,
    Json(request): Json<SaveFeedSourceRequest>,
) -> Result<Json<ApiResponse<FeedSourceView>>, ApiError> {
    save_single_feed(state, request.into_domain_with_id(feed_id)).await
}

/// 删除单个空间 RSS 来源。
#[utoipa::path(
    delete,
    path = "/api/v1/space/feeds/{feed_id}",
    security(("bearer_auth" = [])),
    params(("feed_id" = String, Path, description = "RSS 来源标识")),
    responses((status = 200, description = "删除成功。"))
)]
pub async fn delete_feed(
    State(state): State<Arc<AppState>>,
    Path(feed_id): Path<String>,
) -> Result<Json<ApiResponse<DeleteFeedSourceResponse>>, ApiError> {
    let outcome = state
        .feed_service
        .delete_feed(state.admin_space_id, FeedSourceId(feed_id))
        .await?;
    Ok(Json(ApiResponse::ok(DeleteFeedSourceResponse {
        id: outcome.source_id.0,
    })))
}

async fn save_single_feed(
    state: Arc<AppState>,
    source: FeedSource,
) -> Result<Json<ApiResponse<FeedSourceView>>, ApiError> {
    let outcome = state
        .feed_service
        .save_feed(state.admin_space_id, source)
        .await?;
    Ok(Json(ApiResponse::ok(outcome.source.into())))
}
