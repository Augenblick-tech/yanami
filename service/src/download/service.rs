use std::sync::Arc;

use domain::user::UserId;

use crate::{
    download::shared::error::ApplicationError,
    download::{
        contracts::{DownloadConfiguration, QbitProfileView, UserDownloads as UserDownloadsPort},
        downloads::UserDownloads,
    },
};

/// 用户当前下载配置查询结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserDownloadConfigurationOutcome {
    /// 用户标识。
    pub user_id: UserId,
    /// 当前绑定的下载器标识。
    pub driver_key: Option<String>,
    /// 当前 qbit 配置摘要。
    pub qbit_profile: Option<UserQbitDownloadProfileView>,
    /// 当前系统可用的下载驱动列表。
    pub available_drivers: Vec<String>,
}

/// qbit 配置查询视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserQbitDownloadProfileView {
    /// qbit 地址。
    pub endpoint: String,
    /// qbit 用户名。
    pub username: String,
    /// 下载根路径。
    pub download_path: String,
    /// 是否已经配置密码。
    pub secret_configured: bool,
}

/// 下载应用服务。
pub struct DownloadService {
    downloads: Arc<dyn UserDownloadsPort>,
}

impl DownloadService {
    /// 创建下载应用服务。
    pub fn new(downloads: Arc<UserDownloads>) -> Self {
        Self { downloads }
    }

    #[cfg(test)]
    fn new_for_test(downloads: Arc<dyn UserDownloadsPort>) -> Self {
        Self { downloads }
    }

    /// 查询用户当前绑定的下载器与 qbit 配置摘要。
    pub async fn get_user_download_configuration(
        &self,
        user_id: UserId,
    ) -> Result<GetUserDownloadConfigurationOutcome, ApplicationError> {
        Ok(GetUserDownloadConfigurationOutcome::from(
            self.downloads.get_configuration(user_id).await?,
        ))
    }

    /// 查询当前系统可用的下载驱动标识列表。
    pub async fn list_available_drivers(&self) -> Result<Vec<String>, ApplicationError> {
        self.downloads.available_drivers().await.map_err(Into::into)
    }

    /// 保存用户当前下载器选择，并立即清理运行时缓存。
    pub async fn select_user_download_driver(
        &self,
        user_id: UserId,
        driver_key: String,
    ) -> Result<String, ApplicationError> {
        self.downloads
            .select_driver(user_id, driver_key)
            .await
            .map_err(Into::into)
    }

    /// 保存用户 qbit 配置，并在保存前执行连通性校验。
    pub async fn save_user_qbit_profile(
        &self,
        user_id: UserId,
        endpoint: String,
        username: String,
        secret: String,
        download_path: String,
    ) -> Result<(), ApplicationError> {
        self.downloads
            .save_qbit_profile(user_id, endpoint, username, secret, download_path)
            .await
            .map_err(Into::into)
    }

    /// 向单个用户发起下载请求。
    pub async fn download_for_user(
        &self,
        user_id: UserId,
        source_url: String,
        resource_id: String,
        relative_save_path: String,
    ) -> Result<(), ApplicationError> {
        self.downloads
            .download_for_user(user_id, source_url, resource_id, relative_save_path)
            .await?;
        Ok(())
    }
}

impl From<DownloadConfiguration> for GetUserDownloadConfigurationOutcome {
    fn from(configuration: DownloadConfiguration) -> Self {
        Self {
            user_id: configuration.user_id,
            driver_key: configuration.driver_key,
            qbit_profile: configuration.qbit_profile.map(Into::into),
            available_drivers: configuration.available_drivers,
        }
    }
}

impl From<QbitProfileView> for UserQbitDownloadProfileView {
    fn from(profile: QbitProfileView) -> Self {
        Self {
            endpoint: profile.endpoint,
            username: profile.username,
            download_path: profile.download_path,
            secret_configured: profile.secret_configured,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::{shared::error::DomainError, user::UserId};

    use super::*;
    use crate::download::contracts::{
        DownloadConfiguration, QbitProfileView, UserDownloads as UserDownloadsPort,
    };

    type SavedQbitProfile = (UserId, String, String, String, String);

    #[derive(Default)]
    struct DownloadState {
        selected_driver: Mutex<Vec<(UserId, String)>>,
        saved_qbit: Mutex<Vec<SavedQbitProfile>>,
        user_downloads: Mutex<Vec<(UserId, String, String, String)>>,
    }

    struct StubDownloads {
        configuration: DownloadConfiguration,
        state: Arc<DownloadState>,
    }

    #[async_trait]
    impl UserDownloadsPort for StubDownloads {
        async fn get_configuration(
            &self,
            _user_id: UserId,
        ) -> Result<DownloadConfiguration, DomainError> {
            Ok(self.configuration.clone())
        }

        async fn available_drivers(&self) -> Result<Vec<String>, DomainError> {
            Ok(self.configuration.available_drivers.clone())
        }

        async fn select_driver(
            &self,
            user_id: UserId,
            driver_key: String,
        ) -> Result<String, DomainError> {
            self.state
                .selected_driver
                .lock()
                .expect("selected_driver")
                .push((user_id, driver_key.clone()));
            Ok(driver_key)
        }

        async fn save_qbit_profile(
            &self,
            user_id: UserId,
            endpoint: String,
            username: String,
            secret: String,
            download_path: String,
        ) -> Result<(), DomainError> {
            self.state.saved_qbit.lock().expect("saved_qbit").push((
                user_id,
                endpoint,
                username,
                secret,
                download_path,
            ));
            Ok(())
        }

        async fn download_for_user(
            &self,
            user_id: UserId,
            source_url: String,
            resource_id: String,
            relative_save_path: String,
        ) -> Result<(), DomainError> {
            self.state
                .user_downloads
                .lock()
                .expect("user_downloads")
                .push((user_id, source_url, resource_id, relative_save_path));
            Ok(())
        }
    }

    fn service(state: Arc<DownloadState>, configuration: DownloadConfiguration) -> DownloadService {
        DownloadService::new_for_test(Arc::new(StubDownloads {
            configuration,
            state,
        }))
    }

    #[tokio::test]
    async fn gets_user_download_configuration() {
        let service = service(
            Arc::new(DownloadState::default()),
            DownloadConfiguration {
                user_id: UserId(7),
                driver_key: Some("qbit".to_string()),
                qbit_profile: Some(QbitProfileView {
                    endpoint: "http://127.0.0.1:8080".to_string(),
                    username: "alice".to_string(),
                    download_path: "/data/downloads".to_string(),
                    secret_configured: true,
                }),
                available_drivers: vec![],
            },
        );

        let outcome = service
            .get_user_download_configuration(UserId(7))
            .await
            .expect("configuration");

        assert_eq!(outcome.user_id, UserId(7));
        assert_eq!(outcome.driver_key.as_deref(), Some("qbit"));
        assert_eq!(
            outcome.qbit_profile.expect("qbit").download_path,
            "/data/downloads"
        );
        assert!(outcome.available_drivers.is_empty());
    }

    #[tokio::test]
    async fn delegates_configuration_changes_and_download_commands() {
        let state = Arc::new(DownloadState::default());
        let service = service(
            Arc::clone(&state),
            DownloadConfiguration {
                user_id: UserId(9),
                driver_key: None,
                qbit_profile: None,
                available_drivers: vec![],
            },
        );

        let selected = service
            .select_user_download_driver(UserId(9), "qbit".to_string())
            .await
            .expect("select driver");
        assert_eq!(selected, "qbit");

        service
            .save_user_qbit_profile(
                UserId(9),
                "http://127.0.0.1:8080".to_string(),
                "bob".to_string(),
                "secret".to_string(),
                "/downloads".to_string(),
            )
            .await
            .expect("save qbit");
        service
            .download_for_user(
                UserId(9),
                "magnet:?xt=urn:btih:abc".to_string(),
                "abc".to_string(),
                "Anime/Season 1".to_string(),
            )
            .await
            .expect("download user");

        assert_eq!(
            state
                .selected_driver
                .lock()
                .expect("selected_driver")
                .as_slice(),
            &[(UserId(9), "qbit".to_string())]
        );
        assert_eq!(state.saved_qbit.lock().expect("saved_qbit").len(), 1);
        assert_eq!(
            state.user_downloads.lock().expect("user_downloads").len(),
            1
        );
    }
}
