use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{AccessTokenClaims, ApiResponse, SystemStatResponse},
};
use axum::{extract::State, Extension, Json};
use feed::entity::cap::FeedAccessPolicy;
use std::sync::Arc;

/// 获取系统统计信息
#[utoipa::path(
    get,
    path = "/api/v1/stat",
    operation_id = "stat_get_system_stat",
    tag = "Stat",
    summary = "获取系统统计信息",
    description = "获取整个系统及当前用户的相关订阅统计数据。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    responses(
        (status = 200, description = "获取成功。", body = ApiResponse<SystemStatResponse>),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_system_stat(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
) -> Result<Json<ApiResponse<SystemStatResponse>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let backoff_ids = ctx.caps.access_policy.block_feed_details();
    let stat = ctx
        .queries
        .stat_view
        .get_system_stat(user_entity.space_id(), &backoff_ids)
        .await?;

    Ok(Json(ApiResponse::ok(stat)))
}
