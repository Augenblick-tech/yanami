use async_trait::async_trait;

use crate::{shared::error::DomainError, user::UserId, user::UserRole};

pub struct UserInfoUpdate {
    pub username: String,
    pub role: UserRole,
}

#[async_trait]
pub trait UserInfoWriterCap: Send + Sync {
    async fn write_info(&self, user_id: UserId, info: &UserInfoUpdate) -> Result<(), DomainError>;
}

#[async_trait]
pub trait UserPasswordChangerCap: Send + Sync {
    async fn write_password(&self, user_id: UserId, new_hash: String) -> Result<(), DomainError>;
}
