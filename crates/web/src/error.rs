use anyhow::Error as AnyhowError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use common::shared::error::Error;
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

/// HTTP API 业务码定义。
///
/// 编码规则：
/// - `00-xxx` 为系统级通用码，最终以三位整型返回，例如成功 `200`
/// - `MM-xxx` 为业务模块码，前两位 `MM` 表示模块，后三位 `xxx` 表示错误语义
/// - 例如：用户模块 `10` 的密码错误 `401`，最终业务码为 `10401`
///
/// 模块划分：
/// - `00` 系统与协议层：认证、鉴权、网关、内部错误
/// - `10` 用户与认证：注册、登录、密码、注册码
/// - `20` 番剧：番剧目录、元数据、追踪状态
/// - `30` 协作：团队、成员、订阅空间
/// - `40` Feed：RSS 源、采集输入校验
/// - `60` 下载：下载器、qbit、保存路径
pub mod code {
    pub const NOT_FOUND: i32 = 404;
    pub const FORBIDDEN: i32 = 403;
    pub const INTERNAL_ERROR: i32 = 500;

    pub const IDENTITY_PASSWORD_MISMATCH: i32 = make(MODULE_IDENTITY, 401);
    const MODULE_SYSTEM: i32 = 0;
    const MODULE_IDENTITY: i32 = 10;

    const fn make(module: i32, suffix: i32) -> i32 {
        if module == MODULE_SYSTEM {
            suffix
        } else {
            module * 1000 + suffix
        }
    }
}

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

    /// 返回请求参数错误
    pub fn invalid_request() -> Self {
        Self::new(StatusCode::OK, 500, "invalid request")
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::InvariantViolation(msg) => ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: 50001,
                message: msg,
            },
            Error::NotFound(msg) => ApiError {
                status: StatusCode::NOT_FOUND,
                code: 40401,
                message: msg,
            },
            Error::Conflict(msg) => ApiError {
                status: StatusCode::CONFLICT,
                code: 40901,
                message: msg,
            },
            Error::ExternalContractMismatch { context, source } => ApiError {
                status: StatusCode::BAD_GATEWAY,
                code: 50201,
                message: format!("{}: {}", context, source),
            },
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
