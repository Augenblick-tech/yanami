use async_trait::async_trait;
use domain::shared::error::DomainError;

use crate::entity::user_entity::UserEntity;

pub trait UserPasswordGenerator: Send + Sync {
    fn generator(&self, raw_pwd: &str) -> Result<String, DomainError>;
    fn verify(&self, raw_pwd: &str, pwd_hash: &str) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait UserRepository: Send + Sync + UserIDGenerator {
    async fn find(&self, user_id : u32) -> Result<Option<UserEntity>, DomainError>;
    async fn insert(&self, user: &UserEntity) -> Result<(), DomainError>;
    async fn update(&self, user: &UserEntity) -> Result<(), DomainError>;
}

#[async_trait]
pub trait UserIDGenerator: Send + Sync {
    async fn next_id(&self) -> Result<u32, DomainError>;
}
