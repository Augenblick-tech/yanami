use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    Json,
};
use service::download::service::{
    GetUserDownloadConfigurationOutcome, UserQbitDownloadProfileView,
};

use crate::http::{auth::AuthenticatedUser, error::ApiError, model::*, state::AppState};

/// 查询下载配置，包含当前已选驱动、qBittorrent 配置摘要和系统可用的驱动列表。
#[utoipa::path(
    get,
    path = "/api/v1/users/me/download",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "下载配置查询成功，available_drivers 返回系统当前已注册的下载驱动标识列表。"))
)]
pub async fn get_download_configuration(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<ApiResponse<DownloadConfigurationResponse>>, ApiError> {
    let outcome = state
        .download_service
        .get_user_download_configuration(user.user_id)
        .await?;
    Ok(Json(ApiResponse::ok(download_configuration_response(
        outcome,
    ))))
}

/// 选择下载驱动。可用标识（如 "qbit"、"noop"）可通过 GET /api/v1/users/me/download 的 available_drivers 字段获取。
#[utoipa::path(
    put,
    path = "/api/v1/users/me/download/driver",
    security(("bearer_auth" = [])),
    request_body = SelectDownloadDriverRequest,
    responses((status = 200, description = "驱动切换成功。"))
)]
pub async fn select_download_driver(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<SelectDownloadDriverRequest>,
) -> Result<Json<ApiResponse<DriverResponse>>, ApiError> {
    state
        .download_service
        .select_user_download_driver(user.user_id, request.driver_key.clone())
        .await?;
    Ok(Json(ApiResponse::ok(DriverResponse {
        driver_key: request.driver_key,
    })))
}

/// 保存 qBittorrent 配置。
#[utoipa::path(
    put,
    path = "/api/v1/users/me/download/qbit",
    security(("bearer_auth" = [])),
    request_body = SaveQbitProfileRequest,
    responses((status = 200, description = "配置保存成功。"))
)]
pub async fn save_qbit_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<SaveQbitProfileRequest>,
) -> Result<Json<ApiResponse<QbitProfileResponse>>, ApiError> {
    state
        .download_service
        .save_user_qbit_profile(
            user.user_id,
            request.endpoint.clone(),
            request.username.clone(),
            request.password.clone(),
            request.download_path.clone(),
        )
        .await?;
    Ok(Json(ApiResponse::ok(QbitProfileResponse {
        endpoint: request.endpoint,
        username: request.username,
        download_path: request.download_path,
    })))
}

fn download_configuration_response(
    outcome: GetUserDownloadConfigurationOutcome,
) -> DownloadConfigurationResponse {
    DownloadConfigurationResponse {
        driver_key: outcome.driver_key,
        qbit_profile: outcome.qbit_profile.map(qbit_profile_response),
        available_drivers: outcome.available_drivers,
    }
}

fn qbit_profile_response(profile: UserQbitDownloadProfileView) -> QbitProfileResponse {
    QbitProfileResponse {
        endpoint: profile.endpoint,
        username: profile.username,
        download_path: profile.download_path,
    }
}

#[cfg(test)]
mod tests {
    use domain::user::UserId;

    use super::*;

    #[test]
    fn download_configuration_response_preserves_empty_configuration() {
        let response = download_configuration_response(GetUserDownloadConfigurationOutcome {
            user_id: UserId(7),
            driver_key: None,
            qbit_profile: None,
            available_drivers: vec!["qbit".to_string()],
        });

        assert_eq!(response.driver_key, None);
        assert!(response.qbit_profile.is_none());
        assert_eq!(response.available_drivers, vec!["qbit"]);
    }

    #[test]
    fn download_configuration_response_preserves_configured_values() {
        let response = download_configuration_response(GetUserDownloadConfigurationOutcome {
            user_id: UserId(7),
            driver_key: Some("qbit".to_string()),
            qbit_profile: Some(UserQbitDownloadProfileView {
                endpoint: "http://127.0.0.1:8080".to_string(),
                username: "alice".to_string(),
                download_path: "/downloads".to_string(),
                secret_configured: true,
            }),
            available_drivers: vec!["qbit".to_string(), "noop".to_string()],
        });

        assert_eq!(response.driver_key.as_deref(), Some("qbit"));
        let profile = response.qbit_profile.expect("qbit profile");
        assert_eq!(profile.endpoint, "http://127.0.0.1:8080");
        assert_eq!(profile.username, "alice");
        assert_eq!(profile.download_path, "/downloads");
        assert_eq!(
            response.available_drivers,
            vec!["qbit".to_string(), "noop".to_string()]
        );
    }
}
