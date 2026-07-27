use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    app_ctx::AppContext,
    error::ApiError,
    model::{
        AccessTokenClaims, ApiResponse, DownloadTaskAction, DownloadTaskActionRequest,
        DownloadTaskResponse,
    },
};
use user::entity::cap::DownloaderManager;
use user::entity::model::DownloaderConfig;

/// 获取默认下载器的所有任务列表
#[utoipa::path(
    get,
    path = "/api/v1/downloader/tasks",
    operation_id = "downloader_list_tasks",
    tag = "Downloader",
    summary = "获取默认下载器任务列表",
    description = "获取默认下载器当前的所有下载任务列表。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    responses(
        (status = 200, description = "获取成功，返回任务列表的 JSON 数据"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误或内置下载器未配置")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn list_tasks(
    State(ctx): State<Arc<AppContext>>,
    axum::Extension(user): axum::Extension<AccessTokenClaims>,
) -> Result<Json<ApiResponse<Vec<DownloadTaskResponse>>>, ApiError> {
    let provider = get_provider(&ctx, user.user_id).await?;

    let tasks = provider
        .list_task()
        .await
        .map_err(|_| ApiError::business(60500, "list tasks failed"))?;

    let responses: Vec<DownloadTaskResponse> = tasks
        .into_iter()
        .map(|t| DownloadTaskResponse {
            hash: hex::encode(t.hash),
            name: t.name,
            state: t.state.to_string(),
            progress: t.progress,
            total_size: t.total_size,
            download_speed: t.download_speed,
            is_seeding: t.is_seeding,
            upload_speed: t.upload_speed,
            seed_ratio: t.seed_ratio,
            seed_duration: t.seed_duration,
        })
        .collect();

    Ok(Json(ApiResponse::ok(responses)))
}

/// 修改默认下载器中的某个下载任务状态（如暂停或恢复）
#[utoipa::path(
    put,
    path = "/api/v1/downloader/tasks/{hash}",
    operation_id = "downloader_update_task_state",
    tag = "Downloader",
    summary = "修改指定下载任务状态",
    description = "根据传入的资源 Hash，修改默认下载器中对应的下载任务状态（如暂停或恢复）。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("hash" = String, Path, description = "需要操作的任务的 Hash (Hex 格式)")
    ),
    request_body = DownloadTaskActionRequest,
    responses(
        (status = 200, description = "操作成功"),
        (status = 400, description = "请求参数错误 (如 Hash 格式无效)"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误或内置下载器未配置")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn update_task_state(
    State(ctx): State<Arc<AppContext>>,
    axum::Extension(user): axum::Extension<AccessTokenClaims>,
    Path(hash): Path<String>,
    Json(req): Json<DownloadTaskActionRequest>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let (provider, parsed_hash) = get_provider_and_hash(&ctx, user.user_id, &hash).await?;
    match req.action {
        DownloadTaskAction::Pause => {
            provider
                .pause_task(parsed_hash)
                .await
                .map_err(|_| ApiError::business(60500, "pause task failed"))?;
        }
        DownloadTaskAction::Resume => {
            provider
                .resume_task(parsed_hash)
                .await
                .map_err(|_| ApiError::business(60500, "resume task failed"))?;
        }
    }
    Ok(Json(ApiResponse::ok(())))
}

/// 删除默认下载器中的某个下载任务
#[utoipa::path(
    delete,
    path = "/api/v1/downloader/tasks/{hash}",
    operation_id = "downloader_delete_task",
    tag = "Downloader",
    summary = "删除指定下载任务",
    description = "根据传入的资源 Hash，删除默认下载器中对应的下载任务及相关文件。\n\n调用此接口需要在请求头中携带有效的 JWT Token。",
    params(
        ("hash" = String, Path, description = "需要删除的任务的 Hash (Hex 格式)")
    ),
    responses(
        (status = 200, description = "操作成功"),
        (status = 400, description = "请求参数错误 (如 Hash 格式无效)"),
        (status = 401, description = "未授权：未提供 Token，或 Token 已过期/无效"),
        (status = 500, description = "服务器内部错误或内置下载器未配置")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn delete_task(
    State(ctx): State<Arc<AppContext>>,
    axum::Extension(user): axum::Extension<AccessTokenClaims>,
    Path(hash): Path<String>,
) -> Result<Json<ApiResponse<()>>, ApiError> {
    let (provider, parsed_hash) = get_provider_and_hash(&ctx, user.user_id, &hash).await?;
    provider
        .delete_task(parsed_hash)
        .await
        .map_err(|_| ApiError::business(60500, "delete task failed"))?;
    Ok(Json(ApiResponse::ok(())))
}

fn parse_hash(hash_str: &str) -> Result<[u8; 20], ApiError> {
    let mut hash = [0u8; 20];
    let bytes = hex::decode(hash_str).map_err(|_| ApiError::invalid_request())?;
    if bytes.len() != 20 {
        return Err(ApiError::invalid_request());
    }
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

async fn get_provider(
    ctx: &Arc<AppContext>,
    user_id: i64,
) -> Result<Arc<dyn user::entity::cap::DownloadProvider>, ApiError> {
    let user_entity = ctx
        .roots
        .users
        .get(user_id)
        .await
        .map_err(|_| ApiError::business(60500, "failed to get user"))?
        .ok_or_else(|| ApiError::unauthorized("user not found"))?;

    let config = user_entity
        .download_config()
        .map_err(|_| ApiError::business(60500, "failed to parse download config"))?
        .ok_or_else(|| ApiError::business(60405, "default downloader is not enabled"))?;

    if !matches!(config, DownloaderConfig::Default(_)) {
        return Err(ApiError::business(
            60405,
            "default downloader is not enabled",
        ));
    }

    let provider = ctx
        .caps
        .downloader_manager
        .get(user_id, &config)
        .await
        .map_err(|_| ApiError::business(60500, "failed to get downloader instance"))?;

    Ok(provider)
}

async fn get_provider_and_hash(
    ctx: &Arc<AppContext>,
    user_id: i64,
    hash_str: &str,
) -> Result<(Arc<dyn user::entity::cap::DownloadProvider>, [u8; 20]), ApiError> {
    let provider = get_provider(ctx, user_id).await?;
    let hash = parse_hash(hash_str)?;
    Ok((provider, hash))
}
