use std::sync::Arc;

use domain::{shared::error::DomainError, user::UserId};
use user::users::Users;

use crate::download::{
    contracts::{DownloadConfiguration, QbitProfileView, UserDownloads as UserDownloadsPort},
    runtime::{
        InvalidateUserDownloadRuntime, UserDownloadDriverBindingStore, UserQbitDownloadProfile,
        UserQbitDownloadProfileStore, VerifyQbitProfile,
    },
    user_actions::{UserDownload, UserDownloadRequest},
};

/// 下载聚合根集合入口。
pub struct UserDownloads {
    users: Arc<Users>,
    drivers: Arc<dyn UserDownloadDriverBindingStore>,
    qbit_profiles: Arc<dyn UserQbitDownloadProfileStore>,
    verify_qbit_profile: Arc<VerifyQbitProfile>,
    invalidate_user_runtime: Arc<InvalidateUserDownloadRuntime>,
    user_download: Arc<UserDownload>,
    available_drivers: Vec<String>,
}

impl UserDownloads {
    /// 创建下载聚合根集合入口。
    pub fn new(
        users: Arc<Users>,
        drivers: Arc<dyn UserDownloadDriverBindingStore>,
        qbit_profiles: Arc<dyn UserQbitDownloadProfileStore>,
        verify_qbit_profile: Arc<VerifyQbitProfile>,
        invalidate_user_runtime: Arc<InvalidateUserDownloadRuntime>,
        user_download: Arc<UserDownload>,
        available_drivers: Vec<String>,
    ) -> Self {
        Self {
            users,
            drivers,
            qbit_profiles,
            verify_qbit_profile,
            invalidate_user_runtime,
            user_download,
            available_drivers,
        }
    }

    async fn ensure_user_exists(&self, user_id: UserId) -> Result<(), DomainError> {
        self.users.load(user_id).await?;
        Ok(())
    }

    pub async fn get_configuration(
        &self,
        user_id: UserId,
    ) -> Result<DownloadConfiguration, DomainError> {
        self.ensure_user_exists(user_id).await?;
        let driver_key = self
            .drivers
            .find_driver_key(user_id)
            .await
            .map_err(DomainError::from)?;
        let qbit_profile = self
            .qbit_profiles
            .find_qbit_profile(user_id)
            .await
            .map_err(DomainError::from)?
            .map(|profile| QbitProfileView {
                endpoint: profile.endpoint,
                username: profile.username,
                download_path: profile.download_path,
                secret_configured: !profile.secret.is_empty(),
            });

        Ok(DownloadConfiguration {
            user_id,
            driver_key,
            qbit_profile,
            available_drivers: self.available_drivers.clone(),
        })
    }

    pub async fn select_driver(
        &self,
        user_id: UserId,
        driver_key: String,
    ) -> Result<String, DomainError> {
        self.ensure_user_exists(user_id).await?;
        let driver_key = validate_driver_key(&driver_key)?;
        self.drivers
            .save_driver_key(user_id, &driver_key)
            .await
            .map_err(DomainError::from)?;
        (self.invalidate_user_runtime)(user_id);
        Ok(driver_key)
    }

    pub async fn save_qbit_profile(
        &self,
        user_id: UserId,
        endpoint: String,
        username: String,
        secret: String,
        download_path: String,
    ) -> Result<(), DomainError> {
        self.ensure_user_exists(user_id).await?;
        let profile = validate_qbit_profile(&endpoint, &username, &secret, &download_path)?;
        (self.verify_qbit_profile)(profile.clone())
            .await
            .map_err(DomainError::from)?;
        self.qbit_profiles
            .save_qbit_profile(user_id, &profile)
            .await
            .map_err(DomainError::from)?;
        (self.invalidate_user_runtime)(user_id);
        Ok(())
    }

    pub async fn download_for_user(
        &self,
        user_id: UserId,
        source_url: String,
        resource_id: String,
        relative_save_path: String,
    ) -> Result<(), DomainError> {
        self.ensure_user_exists(user_id).await?;
        self.user_download
            .download(UserDownloadRequest {
                user_id,
                source_url,
                resource_id,
                relative_save_path,
            })
            .await
            .map(|_| ())
            .map_err(DomainError::from)
    }
}

#[async_trait::async_trait]
impl UserDownloadsPort for UserDownloads {
    async fn get_configuration(
        &self,
        user_id: UserId,
    ) -> Result<DownloadConfiguration, DomainError> {
        Self::get_configuration(self, user_id).await
    }

    async fn available_drivers(&self) -> Result<Vec<String>, DomainError> {
        Ok(self.available_drivers.clone())
    }

    async fn select_driver(
        &self,
        user_id: UserId,
        driver_key: String,
    ) -> Result<String, DomainError> {
        Self::select_driver(self, user_id, driver_key).await
    }

    async fn save_qbit_profile(
        &self,
        user_id: UserId,
        endpoint: String,
        username: String,
        secret: String,
        download_path: String,
    ) -> Result<(), DomainError> {
        Self::save_qbit_profile(self, user_id, endpoint, username, secret, download_path).await
    }

    async fn download_for_user(
        &self,
        user_id: UserId,
        source_url: String,
        resource_id: String,
        relative_save_path: String,
    ) -> Result<(), DomainError> {
        Self::download_for_user(self, user_id, source_url, resource_id, relative_save_path).await
    }
}

fn validate_driver_key(driver_key: &str) -> Result<String, DomainError> {
    let trimmed = driver_key.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvariantViolation(
            "download driver key cannot be empty",
        ));
    }
    if !trimmed
        .chars()
        .all(|char| char.is_ascii_lowercase() || char.is_ascii_digit() || matches!(char, '_' | '-'))
    {
        return Err(DomainError::InvariantViolation(
            "download driver key is invalid",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_qbit_profile(
    endpoint: &str,
    username: &str,
    secret: &str,
    download_path: &str,
) -> Result<UserQbitDownloadProfile, DomainError> {
    use std::path::{Component, Path};

    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(DomainError::InvariantViolation(
            "qbit endpoint cannot be empty",
        ));
    }
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(DomainError::InvariantViolation("qbit endpoint is invalid"));
    }

    let username = username.trim();
    if username.is_empty() {
        return Err(DomainError::InvariantViolation(
            "qbit username cannot be empty",
        ));
    }

    let secret = secret.trim();
    if secret.is_empty() {
        return Err(DomainError::InvariantViolation(
            "qbit secret cannot be empty",
        ));
    }

    let download_path = download_path.trim();
    if download_path.is_empty() {
        return Err(DomainError::InvariantViolation(
            "qbit download path cannot be empty",
        ));
    }
    let download_path_value = Path::new(download_path);
    if !download_path_value.is_absolute()
        || download_path_value
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(DomainError::InvariantViolation(
            "qbit download path is invalid",
        ));
    }

    Ok(UserQbitDownloadProfile {
        endpoint: endpoint.to_string(),
        username: username.to_string(),
        secret: secret.to_string(),
        download_path: download_path.to_string(),
    })
}
