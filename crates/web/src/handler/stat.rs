use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{AccessTokenClaims, ApiResponse, LogLevelRequest, SystemStatResponse},
};
use axum::{Extension, Json, extract::State};
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

/// 修改系统日志级别
#[utoipa::path(
    put,
    path = "/api/v1/system/log-level",
    operation_id = "stat_set_log_level",
    tag = "Stat",
    summary = "修改系统日志级别",
    description = "在运行时动态修改系统的日志输出级别。支持 env_filter 的语法，例如 'debug' 或 'hyper=info,my_crate=trace'。\n\n调用此接口需要在请求头中携带有效的 JWT Token，且需要 Admin 权限。",
    request_body = LogLevelRequest,
    responses(
        (status = 200, description = "修改成功。"),
        (status = 400, description = "参数错误：非法的日志级别字符串格式"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：非管理员用户"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn set_log_level(
    State(ctx): State<Arc<AppContext>>,
    Json(req): Json<LogLevelRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    if let Err(e) = (ctx.caps.log_level_reloader)(req.level.clone()) {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            400,
            format!("failed to reload log level: {}", e),
        ));
    }
    tracing::info!("log level reloaded to: {}", req.level);
    Ok(Json(ApiResponse::ok(())))
}
