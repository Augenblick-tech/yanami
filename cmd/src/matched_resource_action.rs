use std::sync::Arc;

use service::download::user_actions::{UserDownload, UserDownloadRequest};
use subscription::{action::MatchedResource, shared::error::ApplicationError};

pub struct DownloadMatchedResourceAction {
    user_download: Arc<UserDownload>,
}

impl DownloadMatchedResourceAction {
    pub fn new(user_download: Arc<UserDownload>) -> Self {
        Self { user_download }
    }

    pub async fn run(&self, resource: MatchedResource) -> Result<(), ApplicationError> {
        self.user_download
            .download(UserDownloadRequest {
                user_id: resource.user_id,
                source_url: resource.source_url,
                resource_id: resource.resource_id,
                relative_save_path: resource.relative_save_path,
            })
            .await
            .map(|_| ())
            .map_err(map_download_error)
    }
}

fn map_download_error(
    error: service::download::shared::error::ApplicationError,
) -> ApplicationError {
    match error {
        service::download::shared::error::ApplicationError::Domain(error) => error.into(),
        service::download::shared::error::ApplicationError::Infrastructure(error) => error.into(),
    }
}
