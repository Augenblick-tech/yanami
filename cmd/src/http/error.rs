use anyhow::Error as AnyhowError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

use domain::shared::error::DomainError;
use service::download::shared::error::ApplicationError as DownloadApplicationError;
use service::shared::error::ApplicationError;

use crate::http::error_code::{code, invariant_violation_code};

/// HTTP 错误响应。
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// 业务码。
    pub code: i32,
    /// 面向调用方的错误消息。
    pub message: String,
    /// 失败时固定为 `null`，保持响应结构稳定。
    pub data: Option<()>,
}

/// HTTP API 统一错误类型。
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: i32,
    message: String,
}

impl ApiError {
    /// 构造一个指定状态码的 HTTP 错误。
    pub fn new(status: StatusCode, code: i32, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// 构造一个业务错误。
    /// 业务错误使用 HTTP 200，通过 `code` 区分失败原因。
    pub fn business(code: i32, message: impl Into<String>) -> Self {
        Self::new(StatusCode::OK, code, message)
    }

    /// 返回 401 未认证错误。
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            code::IDENTITY_PASSWORD_MISMATCH,
            message,
        )
    }

    /// 返回登录凭据错误。
    pub fn invalid_credentials() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            code::IDENTITY_PASSWORD_MISMATCH,
            "invalid username or password",
        )
    }

    /// 返回 403 未授权错误。
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code::FORBIDDEN, message)
    }

    /// 返回 404 资源未找到错误。
    pub fn not_found(entity: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            code::NOT_FOUND,
            format!("{entity} not found"),
        )
    }
}

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::InvariantViolation(message) => map_invariant_violation(message),
            DomainError::ExternalContractMismatch { context, source } => {
                error!(error = ?source, context, "external dependency failed");
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    code::BAD_GATEWAY,
                    "upstream service error",
                )
            }
        }
    }
}

impl From<AnyhowError> for ApiError {
    fn from(error: AnyhowError) -> Self {
        error!(error = ?error, "infrastructure error");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            code::INTERNAL_ERROR,
            "internal server error",
        )
    }
}

impl From<ApplicationError> for ApiError {
    fn from(value: ApplicationError) -> Self {
        match value {
            ApplicationError::Domain(error) => error.into(),
            ApplicationError::Infrastructure(error) => error.into(),
        }
    }
}

impl From<DownloadApplicationError> for ApiError {
    fn from(value: DownloadApplicationError) -> Self {
        match value {
            DownloadApplicationError::Domain(error) => error.into(),
            DownloadApplicationError::Infrastructure(error) => error.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                code: self.code,
                message: self.message,
                data: None,
            }),
        )
            .into_response()
    }
}

fn map_invariant_violation(message: &'static str) -> ApiError {
    match message {
        "admin role is required" | "team owner role is required" => ApiError::forbidden(message),
        _ => ApiError::business(invariant_violation_code(message), message),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    async fn response_json(response: Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        serde_json::from_slice(&body).expect("parse response json")
    }

    #[tokio::test]
    async fn api_error_builders_preserve_http_and_business_semantics() {
        let business = ApiError::business(code::ANIME_NOT_FOUND, "anime not found");
        let invalid_credentials = ApiError::invalid_credentials();
        let forbidden = ApiError::forbidden("admin role is required");

        let business_response = business.into_response();
        let invalid_credentials_response = invalid_credentials.into_response();
        let forbidden_response = forbidden.into_response();

        assert_eq!(business_response.status(), StatusCode::OK);
        assert_eq!(
            invalid_credentials_response.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn domain_errors_map_to_public_responses_without_leaking_internal_details() {
        let invariant = ApiError::from(ApplicationError::Domain(DomainError::InvariantViolation(
            "anime not found",
        )));
        let upstream = ApiError::from(ApplicationError::Domain(DomainError::external(
            "tmdb search failed",
            anyhow::anyhow!("dns failed"),
        )));

        let invariant_response = invariant.into_response();
        let upstream_response = upstream.into_response();

        let invariant_json = response_json(invariant_response).await;
        let upstream_json = response_json(upstream_response).await;

        assert_eq!(
            invariant_json["code"].as_i64(),
            Some(i64::from(code::ANIME_NOT_FOUND))
        );
        assert_eq!(invariant_json["message"].as_str(), Some("anime not found"));
        assert_eq!(
            upstream_json["code"].as_i64(),
            Some(i64::from(code::BAD_GATEWAY))
        );
        assert_eq!(
            upstream_json["message"].as_str(),
            Some("upstream service error")
        );
    }

    #[tokio::test]
    async fn infrastructure_error_maps_to_internal_server_error() {
        let response = ApiError::from(ApplicationError::Infrastructure(anyhow::anyhow!(
            "sqlite busy"
        )))
        .into_response();
        let json = response_json(response).await;

        assert_eq!(json["code"].as_i64(), Some(i64::from(code::INTERNAL_ERROR)));
        assert_eq!(json["message"].as_str(), Some("internal server error"));
        assert!(json["data"].is_null());
    }

    #[tokio::test]
    async fn forbidden_invariant_uses_http_403() {
        let response = ApiError::from(ApplicationError::Domain(DomainError::InvariantViolation(
            "admin role is required",
        )))
        .into_response();
        let status = response.status();
        let json = response_json(response).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(json["code"].as_i64(), Some(i64::from(code::FORBIDDEN)));
    }
}
