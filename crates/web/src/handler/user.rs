use std::sync::Arc;

use axum::{Extension, Json, extract::State};
use user::entity::cap::DownloaderManager;

use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{
        AccessTokenClaims, ApiResponse, AutoSubRequest, AutoSubResponse, ChangePasswordRequest,
        DownloaderSettings, LoginOutcome, LoginRequest, LoginResponse,
    },
};

/// 用户登录。
#[utoipa::path(
    post,
    path = "/api/v1/user/login",
    operation_id = "user_login",
    tag = "User",
    summary = "用户登录",
    description = "使用用户名和明文密码进行登录认证。\n\n成功登录后，将返回包含 `access_token` 的数据。前端需要妥善保存此 Token，并在后续需要鉴权的接口请求头中携带 `Authorization: Bearer <access_token>`。",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "登录成功。返回数据的 `data` 字段为 `LoginResponse` 对象，包含用户的 ID、角色、Token 及过期时间等信息。"),
        (status = 400, description = "请求格式错误或参数校验失败"),
        (status = 401, description = "认证失败：用户名或密码不正确，或用户不存在"),
        (status = 500, description = "服务器内部错误"),
    )
)]
pub async fn login(
    State(ctx): State<Arc<AppContext>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, ApiError> {
    let user = ctx.roots.users.get_by_username(&req.username).await?;
    if let Some(user) = user
        && user.verify_password(&req.password)?
    {
        let token = ctx
            .caps
            .jwt
            .issue_access_token(user.id(), user.role())
            .await?;
        return Ok(Json(ApiResponse::ok(
            LoginOutcome {
                user_id: user.id(),
                role: user.role(),
                access_token: token,
            }
            .into(),
        )));
    }
    Err(ApiError::invalid_credentials())
}

#[utoipa::path(
    get,
    path = "/api/v1/user/download/config",
    operation_id = "user_list_download_config",
    tag = "User",
    summary = "获取当前用户的下载配置列表",
    description = "获取当前登录用户的所有下载工具（如 qBittorrent）配置列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。返回的数据的 `data` 字段是一个数组，包含了各个配置的名称、连接信息以及是否为当前激活（`active`）状态等详细配置信息。",
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `[DownloaderSettings]` 数组。"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_download_config(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
) -> Result<Json<ApiResponse<Vec<DownloaderSettings>>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    let configs = user_entity.get_download_config();
    let configs = configs
        .iter()
        .map(|c| DownloaderSettings::from(c.clone().sanitized()))
        .collect::<Vec<_>>();
    Ok(Json(ApiResponse::ok(configs)))
}

#[utoipa::path(
    post,
    path = "/api/v1/user/download/config",
    operation_id = "user_save_download_config",
    tag = "User",
    summary = "保存或更新下载配置",
    description = "新增或覆盖更新当前用户的下载器配置。\n\n- 如果提交的配置 `name` 已经存在，则会覆盖原有配置（相当于更新）；如果不存在，则新增。\n- 当提交的配置 `active` 字段为 `true` 时，系统会自动将当前用户的其他所有配置的 `active` 状态设为 `false`，确保同一时间只有一个被激活的配置。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = DownloaderSettings,
    responses(
        (status = 200, description = "保存成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败或 JSON 格式错误"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn save_download_config(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<DownloaderSettings>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };
    let config: user::entity::model::DownloaderConfig = req.into();
    ctx.caps
        .downloader_manager
        .validate_config(&config)
        .await
        .map_err(|e| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                400,
                format!("download config validate failed: {}", e),
            )
        })?;

    user_entity.save_download_config(config)?;
    ctx.roots.users.save(&user_entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

#[utoipa::path(
    put,
    path = "/api/v1/user/download/config/active",
    operation_id = "user_switch_active_download_config",
    tag = "User",
    summary = "切换激活的下载配置",
    description = "根据配置名称（`name`）切换当前用户激活的下载配置，该操作不触发配置连通性校验。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body(content = crate::model::SwitchActiveDownloaderRequest, description = "需要激活的下载配置名称"),
    responses(
        (status = 200, description = "切换操作执行成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败，或指定的配置不存在"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 409, description = "指定的下载配置未找到"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn switch_active_download_config(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<crate::model::SwitchActiveDownloaderRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };
    user_entity.enable_download_config(&req.name)?;
    ctx.roots.users.save(&user_entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

#[utoipa::path(
    delete,
    path = "/api/v1/user/download/config/{name}",
    operation_id = "user_delete_download_config",
    tag = "User",
    summary = "删除指定的下载配置",
    description = "根据配置名称（`name`）删除当前用户的某一项下载配置。\n\n如果指定的配置不存在，接口不会报错，而是表现为幂等（直接返回成功）。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("name" = String, Path, description = "需要删除的下载配置的名称 (即配置对象中的 name 字段)")
    ),
    responses(
        (status = 200, description = "删除操作执行成功。返回数据的 `data` 字段为空。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_download_config(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };
    user_entity.delete_download_config(&name);
    ctx.roots.users.save(&user_entity).await?;
    Ok(Json(ApiResponse::ok(())))
}

#[utoipa::path(
    post,
    path = "/api/v1/user/password",
    operation_id = "user_change_password",
    tag = "User",
    summary = "修改密码",
    description = "修改当前登录用户的密码。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "修改成功。"),
        (status = 400, description = "原密码错误或请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn change_password(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    if !user_entity.verify_password(&req.old_password)? {
        return Err(ApiError::invalid_credentials());
    }

    user_entity.set_password(&req.new_password)?;
    ctx.roots.users.save(&user_entity).await?;

    Ok(Json(ApiResponse::ok(())))
}

#[utoipa::path(
    post,
    path = "/api/v1/user/auto_sub",
    operation_id = "user_toggle_auto_sub",
    tag = "User",
    summary = "开关自动订阅",
    description = "开启或关闭当前登录用户的自动订阅功能。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    request_body = AutoSubRequest,
    responses(
        (status = 200, description = "修改成功。"),
        (status = 400, description = "请求参数校验失败"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn toggle_auto_sub(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
    Json(req): Json<AutoSubRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let Some(mut user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    if req.auto_sub {
        user_entity.enable_auto_sub_anime();
    } else {
        user_entity.disable_auto_sub_anime();
    }

    ctx.roots.users.save(&user_entity).await?;

    Ok(Json(ApiResponse::ok(())))
}

#[utoipa::path(
    get,
    path = "/api/v1/user/auto_sub",
    operation_id = "user_get_auto_sub",
    tag = "User",
    summary = "获取自动订阅状态",
    description = "获取当前登录用户的自动订阅状态。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    responses(
        (status = 200, description = "获取成功。返回数据的 `data` 字段为 `AutoSubResponse`。"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 403, description = "禁止访问：Token 鉴权通过但系统中找不到该对应的用户记录"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn get_auto_sub(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AccessTokenClaims>,
) -> Result<Json<ApiResponse<AutoSubResponse>>, ApiError> {
    let Some(user_entity) = ctx.roots.users.get(user.user_id).await? else {
        return Err(ApiError::forbidden("not found user"));
    };

    Ok(Json(ApiResponse::ok(AutoSubResponse {
        auto_sub: user_entity.auto_sub(),
    })))
}
