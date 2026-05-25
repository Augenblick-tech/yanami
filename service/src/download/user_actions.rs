use std::path::{Component, Path};
use std::sync::Arc;

use domain::{
    download::DownloadRequest,
    shared::error::DomainError,
    user::UserId,
};

use crate::download::shared::error::ApplicationError;
use crate::download::{
    contracts::UserDownloadExecutor,
    runtime::RoutingUserDownloadExecutor,
};

/// 一次面向用户的下载请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDownloadRequest {
    pub user_id: UserId,
    pub source_url: String,
    pub resource_id: String,
    pub relative_save_path: String,
}

/// 下载请求执行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDownloadResult {
    pub user_id: UserId,
}

/// 执行用户下载。
pub struct UserDownload {
    executor: Arc<dyn UserDownloadExecutor>,
}

impl UserDownload {
    pub fn new(executor: Arc<RoutingUserDownloadExecutor>) -> Self {
        Self { executor }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(executor: Arc<dyn UserDownloadExecutor>) -> Self {
        Self { executor }
    }

    pub async fn download(
        &self,
        request: UserDownloadRequest,
    ) -> Result<UserDownloadResult, ApplicationError> {
        let download_request = DownloadRequest {
            source_url: request.source_url,
            resource_id: request.resource_id,
            relative_target_path: validate_relative_path(&request.relative_save_path)?,
        };

        self.executor
            .download_for_user(request.user_id, &download_request)
            .await?;

        Ok(UserDownloadResult {
            user_id: request.user_id,
        })
    }
}

fn validate_relative_path(relative_path: &str) -> Result<String, ApplicationError> {
    if relative_path.trim().is_empty() {
        return Err(DomainError::InvariantViolation("relative save path cannot be empty").into());
    }

    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(DomainError::InvariantViolation("relative save path is invalid").into());
    }

    relative.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        DomainError::InvariantViolation("relative save path is not valid utf-8").into()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::{shared::error::DomainError, user::UserId};

    use super::*;
    use crate::download::contracts::UserDownloadExecutor;
    use domain::download::DownloadRequest;

    #[derive(Default)]
    struct RecordingUserDownloadExecutor {
        requests: Mutex<Vec<(UserId, DownloadRequest)>>,
    }

    #[async_trait]
    impl UserDownloadExecutor for RecordingUserDownloadExecutor {
        async fn download_for_user(
            &self,
            user_id: UserId,
            request: &DownloadRequest,
        ) -> Result<(), DomainError> {
            self.requests
                .lock()
                .expect("lock requests")
                .push((user_id, request.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn download_for_user_uses_user_configured_root_path() {
        let executor = Arc::new(RecordingUserDownloadExecutor::default());
        let use_case = UserDownload::new_for_test(executor.clone());

        let outcome = use_case
            .download(UserDownloadRequest {
                user_id: UserId(7),
                source_url: "magnet:?xt=urn:btih:test".to_string(),
                resource_id: "hash".to_string(),
                relative_save_path: "Frieren/S01".to_string(),
            })
            .await
            .expect("download succeeds");

        assert_eq!(outcome.user_id, UserId(7));
        assert_eq!(
            executor.requests.lock().expect("lock requests")[0]
                .1
                .relative_target_path,
            "Frieren/S01"
        );
    }

    #[tokio::test]
    async fn download_for_user_rejects_parent_dir_path() {
        let executor = Arc::new(RecordingUserDownloadExecutor::default());
        let use_case = UserDownload::new_for_test(executor);

        let error = use_case
            .download(UserDownloadRequest {
                user_id: UserId(7),
                source_url: "magnet:?xt=urn:btih:test".to_string(),
                resource_id: "hash".to_string(),
                relative_save_path: "../escape".to_string(),
            })
            .await
            .expect_err("download must fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: relative save path is invalid"
        );
    }
}
