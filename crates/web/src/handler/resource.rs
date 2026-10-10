use std::sync::Arc;

use crate::app_ctx::AppContext;
use crate::{
    error::ApiError,
    model::{AccessTokenClaims, ApiResponse, PageResourceRequest, ResourcePage},
};
use axum::{Extension, Json, extract::State};

/// 获取资源列表
#[utoipa::path(
    post,
    path = "/api/v1/resource/list",
    operation_id = "resource_list",
    tag = "Resource",
    summary = "获取资源列表",
    description = "从资源库里按关键字读取资源，供手动匹配使用。\n\n传了 `sub_anime_id` 时会校验该订阅属于当前用户，并返回资源在该订阅下的集数。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = PageResourceRequest,
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `ResourcePage` 对象。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录或越权操作"),
        (status = 404, description = "资源不存在：未找到关联的订阅记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(request): Json<PageResourceRequest>,
) -> Result<Json<ApiResponse<ResourcePage>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    // 关联订阅只用来标出该订阅已经匹配到的集号，必须是自己空间下的订阅
    if let Some(sub_anime_id) = request.sub_anime_id {
        let Some(entity) = ctx
            .roots
            .sub_animes
            .find_by_sub_anime_id(sub_anime_id)
            .await?
        else {
            return Err(ApiError::not_found("not found subscription"));
        };
        if entity.space_id() != user_entity.space_id() {
            return Err(ApiError::forbidden("forbidden"));
        }
    }

    let page = request.page.unwrap_or(1).max(1);
    let page_size = request.page_size.unwrap_or(20).clamp(1, 100);
    let result = ctx
        .queries
        .resource_view
        .page_resources(&request, page, page_size)
        .await?;

    Ok(Json(ApiResponse::ok(result)))
}
