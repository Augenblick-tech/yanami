use async_trait::async_trait;
use domain::{
    shared::biz::BizContext,
    shared::error::DomainError,
    user::{PasswordHash, RegistrationCodeValue, UserId},
};

#[async_trait]
pub trait PasswordService: Send + Sync {
    async fn hash_password(&self, plaintext: &str) -> Result<PasswordHash, DomainError>;
    async fn verify_password(
        &self,
        password_hash: &PasswordHash,
        plaintext: &str,
    ) -> Result<bool, DomainError>;
}

pub trait EpochClock: Send + Sync {
    fn now_epoch_seconds(&self) -> i64;
}

#[async_trait]
pub trait RegistrationCodeGenerator: Send + Sync {
    async fn generate_registration_code(&self) -> Result<RegistrationCodeValue, DomainError>;
}

#[async_trait]
pub trait UserIdGenerator: Send + Sync {
    async fn next_user_id(&self) -> Result<UserId, DomainError>;

    fn with_biz(&self, _: &BizContext) -> Result<std::sync::Arc<dyn UserIdGenerator>, DomainError> {
        Err(DomainError::InvariantViolation(
            "user id generator does not support biz context",
        ))
    }
}
