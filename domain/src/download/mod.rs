use async_trait::async_trait;

use crate::shared::error::DomainError;
use crate::user::UserId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub source_url: String,
    pub resource_id: String,
    pub relative_target_path: String,
}

#[async_trait]
pub trait UserDownloadDriver: Send + Sync {
    fn driver_key(&self) -> &'static str;
    async fn download(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), DomainError>;
}

#[async_trait]
pub trait UserDownloadDriverBindingStore: Send + Sync {
    async fn find_driver_key(&self, user_id: UserId) -> Result<Option<String>, DomainError>;
    async fn save_driver_key(
        &self,
        user_id: UserId,
        driver_key: &str,
    ) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserQbitDownloadProfile {
    pub endpoint: String,
    pub username: String,
    pub secret: String,
    pub download_path: String,
}

#[async_trait]
pub trait UserQbitDownloadProfileStore: Send + Sync {
    async fn find_qbit_profile(
        &self,
        user_id: UserId,
    ) -> Result<Option<UserQbitDownloadProfile>, DomainError>;
    async fn save_qbit_profile(
        &self,
        user_id: UserId,
        profile: &UserQbitDownloadProfile,
    ) -> Result<(), DomainError>;
}
