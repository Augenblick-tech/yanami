use async_trait::async_trait;

use domain::{shared::error::DomainError, user::UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub source_url: String,
    pub resource_id: String,
    pub relative_target_path: String,
}

#[async_trait]
pub(crate) trait UserDownloadExecutor: Send + Sync {
    async fn download_for_user(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadConfiguration {
    pub user_id: UserId,
    pub driver_key: Option<String>,
    pub qbit_profile: Option<QbitProfileView>,
    pub available_drivers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QbitProfileView {
    pub endpoint: String,
    pub username: String,
    pub download_path: String,
    pub secret_configured: bool,
}

#[async_trait]
pub(crate) trait UserDownloads: Send + Sync {
    async fn get_configuration(
        &self,
        user_id: UserId,
    ) -> Result<DownloadConfiguration, DomainError>;

    async fn available_drivers(&self) -> Result<Vec<String>, DomainError>;

    async fn select_driver(
        &self,
        user_id: UserId,
        driver_key: String,
    ) -> Result<String, DomainError>;

    async fn save_qbit_profile(
        &self,
        user_id: UserId,
        endpoint: String,
        username: String,
        secret: String,
        download_path: String,
    ) -> Result<(), DomainError>;

    async fn download_for_user(
        &self,
        user_id: UserId,
        source_url: String,
        resource_id: String,
        relative_save_path: String,
    ) -> Result<(), DomainError>;
}
