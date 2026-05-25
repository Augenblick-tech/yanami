use async_trait::async_trait;
use domain::{
    download::{DownloadRequest, UserDownloadDriver},
    shared::error::DomainError,
    user::UserId,
};

pub struct NoopDownloadDriver;

#[async_trait]
impl UserDownloadDriver for NoopDownloadDriver {
    fn driver_key(&self) -> &'static str {
        "noop"
    }

    async fn download(
        &self,
        user_id: UserId,
        request: &DownloadRequest,
    ) -> Result<(), DomainError> {
        tracing::info!(
            user_id = %user_id.0,
            resource_id = %request.resource_id,
            source_url = %request.source_url,
            relative_target_path = %request.relative_target_path,
            "noop download driver accepted request"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_driver_accepts_download_request_without_side_effects() {
        let driver = NoopDownloadDriver;

        driver
            .download(
                UserId(7),
                &DownloadRequest {
                    source_url: "magnet:?xt=urn:btih:test".to_string(),
                    resource_id: "resource-1".to_string(),
                    relative_target_path: "Show/Season 1".to_string(),
                },
            )
            .await
            .expect("noop download succeeds");

        assert_eq!(driver.driver_key(), "noop");
    }
}
