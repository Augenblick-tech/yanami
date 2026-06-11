use std::{path::Path, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use common::shared::{cap, error::Error};

use crate::entity::cap::DownloadProvider;

#[derive(Clone)]
pub struct Downloader {
    user_id: i64,
    base_path: String,
    downloader: Arc<dyn DownloadProvider>,
}

impl Downloader {
    pub fn new(
        user_id: i64,
        base_path: String,
        download_provider: Arc<dyn DownloadProvider>,
    ) -> Self {
        Self {
            user_id,
            base_path,
            downloader: download_provider,
        }
    }
}

#[async_trait]
impl cap::Downloader for Downloader {
    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool, Error> {
        let p = Path::new(&self.base_path).join(path);
        let download_path = p
            .to_str()
            .context("not found download path")
            .map_err(|e| Error::conflict(e.to_string()))?;
        let ok = self
            .downloader
            .download(url, download_path, hash)
            .await
            .map_err(|e| {
                Error::external(
                    format!(
                        "user {} use {} download url {} to {} failed",
                        self.user_id,
                        self.downloader.name(),
                        url,
                        download_path
                    ),
                    e,
                )
            })?;
        Ok(ok)
    }
}
