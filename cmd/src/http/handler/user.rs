use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    Json,
};

use crate::http::{auth::AuthenticatedUser, error::ApiError, model::*, state::AppState};
use domain::shared::error::DomainError;
use service::shared::error::ApplicationError;

/// 用户登录。
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "登录成功。"),
        (status = 401, description = "用户名或密码不正确。"),
    )
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, ApiError> {
    let outcome = match state
        .user_service
        .login(request.username, request.password)
        .await
    {
        Ok(outcome) => outcome,
        Err(ApplicationError::Domain(DomainError::InvariantViolation(
            "user not found" | "password does not match",
        ))) => return Err(ApiError::invalid_credentials()),
        Err(error) => return Err(error.into()),
    };
    Ok(Json(ApiResponse::ok(outcome.into())))
}

/// 修改密码。
#[utoipa::path(
    put,
    path = "/api/v1/users/me/password",
    security(("bearer_auth" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "密码修改成功。"),
    )
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthenticatedUser>,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<ChangePasswordResponse>>, ApiError> {
    let outcome = state
        .user_service
        .change_password(_user.user_id, request.old_password, request.new_password)
        .await?;
    Ok(Json(ApiResponse::ok(ChangePasswordResponse {
        user_id: outcome.user_id.0,
    })))
}
