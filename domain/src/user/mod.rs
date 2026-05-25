use std::sync::Arc;

use async_trait::async_trait;

use crate::shared::biz::BizContext;
use crate::shared::error::DomainError;

pub mod capability;

/// 用户稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub i64);

/// 用户登录名。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(pub String);

/// 用户密码摘要。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PasswordHash(pub String);

/// 系统内的用户角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserRole {
    /// 管理员。
    Admin,
    /// 普通用户。
    User,
}

/// 用户账号聚合的共享读写模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// 用户标识。
    pub id: UserId,
    /// 登录名。
    pub username: Username,
    /// 持久化后的密码摘要。
    pub password_hash: PasswordHash,
    /// 账号角色。
    pub role: UserRole,
}

/// 注册码字符串值。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationCodeValue(pub String);

/// 注册码聚合的共享读写模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationCode {
    /// 注册码内容。
    pub code: RegistrationCodeValue,
    /// 签发时的 Unix 时间戳。
    pub issued_at: i64,
    /// 有效时长，单位秒。
    pub valid_for_seconds: i64,
    /// 剩余可用次数。
    pub remaining_uses: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: i64,
}

#[async_trait]
pub trait AccessTokenIssuer: Send + Sync {
    async fn issue_access_token(
        &self,
        user_id: UserId,
        role: UserRole,
    ) -> Result<AccessToken, DomainError>;
}

/// Account context 下的用户账号仓储端口。
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// 按用户标识读取账号。
    async fn find_user(&self, user_id: UserId) -> Result<Option<User>, DomainError>;

    /// 按用户名读取账号。
    async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError>;

    /// 保存账号聚合。
    async fn save_user(&self, user: &User) -> Result<(), DomainError>;

    /// 列出全部用户账号快照。
    async fn list_users(&self) -> Result<Vec<User>, DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn UserRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "user repository does not support biz context",
        ))
    }
}

/// Account context 下的注册码仓储端口。
#[async_trait]
pub trait RegistrationCodeRepository: Send + Sync {
    /// 按注册码内容读取凭证。
    async fn find_registration_code(
        &self,
        code: &RegistrationCodeValue,
    ) -> Result<Option<RegistrationCode>, DomainError>;

    /// 保存注册码聚合。
    async fn save_registration_code(
        &self,
        registration_code: &RegistrationCode,
    ) -> Result<(), DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn RegistrationCodeRepository>, DomainError> {
        Err(DomainError::InvariantViolation(
            "registration code repository does not support biz context",
        ))
    }
}
