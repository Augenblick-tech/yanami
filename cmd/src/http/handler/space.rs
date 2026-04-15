use std::sync::Arc;

use axum::{extract::State, Json};

use crate::http::{error::ApiError, model::*, state::AppState};

/// 查询当前个人空间的自动订阅开关状态。
///
/// 自动订阅开启后，每次番剧日历同步时，新入库的番剧会自动在个人空间中创建启用状态为 true 的订阅。
///
/// ### 响应示例
/// ```json
/// {
///   "code": 0,
///   "data": {
///     "enabled": true
///   }
/// }
/// ```
#[utoipa::path(
    get,
    path = "/api/v1/space/auto-subscribe",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "查询成功", body = ApiResponse<AutoSubscribeResponse>)
    )
)]
pub async fn get_auto_subscribe(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AutoSubscribeResponse>>, ApiError> {
    let enabled = state
        .space_service
        .get_auto_subscribe(state.admin_space_id)
        .await?;
    Ok(Json(ApiResponse::ok(AutoSubscribeResponse { enabled })))
}

/// 设置个人空间的自动订阅开关。
///
/// ### 请求体
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | enabled | bool | 是 | true 开启，false 关闭 |
///
/// ### 请求示例
/// ```json
/// {
///   "enabled": true
/// }
/// ```
///
/// ### 响应示例
/// ```json
/// {
///   "code": 0,
///   "data": {
///     "enabled": true
///   }
/// }
/// ```
#[utoipa::path(
    put,
    path = "/api/v1/space/auto-subscribe",
    security(("bearer_auth" = [])),
    request_body = SetAutoSubscribeRequest,
    responses(
        (status = 200, description = "设置成功", body = ApiResponse<AutoSubscribeResponse>)
    )
)]
pub async fn set_auto_subscribe(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SetAutoSubscribeRequest>,
) -> Result<Json<ApiResponse<AutoSubscribeResponse>>, ApiError> {
    let enabled = state
        .space_service
        .set_auto_subscribe(state.admin_space_id, request.enabled)
        .await?;
    Ok(Json(ApiResponse::ok(AutoSubscribeResponse { enabled })))
}
