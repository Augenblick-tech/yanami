use axum::Json;

use crate::http::model::ApiResponse;

/// 健康检查。
/// 用于容器探针、反向代理探活和前端启动时的基础连通性验证。
/// 该接口只表示 HTTP 进程可用，不保证数据库或上游依赖一定健康。
#[utoipa::path(
    get,
    path = "/api/v1/ping",
    responses((status = 200, description = "服务进程可用。"))
)]
pub async fn ping() -> Json<ApiResponse<String>> {
    Json(ApiResponse::ok("pong".to_string()))
}
