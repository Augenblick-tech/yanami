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

    pub fn provider(&self) -> Arc<dyn DownloadProvider> {
        self.downloader.clone()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::model::DownloadTask;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    struct DownloadCall {
        url: String,
        path: String,
        hash: [u8; 20],
    }

    /// 记录生产代码真正交给 provider 的下载参数。
    #[derive(Default)]
    struct RecordingProvider {
        downloads: Mutex<Vec<DownloadCall>>,
        fail: bool,
    }

    impl RecordingProvider {
        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::default()
            }
        }

        fn last_download(&self) -> DownloadCall {
            self.downloads
                .lock()
                .expect("test lock should not be poisoned")
                .last()
                .cloned()
                .expect("production code should have called download")
        }
    }

    #[async_trait]
    impl DownloadProvider for RecordingProvider {
        fn name(&self) -> &str {
            "recording"
        }

        async fn stop(&self) {}

        async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> anyhow::Result<bool> {
            self.downloads
                .lock()
                .expect("test lock should not be poisoned")
                .push(DownloadCall {
                    url: url.to_string(),
                    path: path.to_string(),
                    hash,
                });
            if self.fail {
                anyhow::bail!("provider fails on purpose");
            }
            Ok(true)
        }

        async fn list_task(&self) -> anyhow::Result<Vec<DownloadTask>> {
            anyhow::bail!("RecordingProvider does not support list_task")
        }

        async fn get_task(&self, hash: [u8; 20]) -> anyhow::Result<Option<DownloadTask>> {
            anyhow::bail!("RecordingProvider does not support get_task {}", hex::encode(hash))
        }

        async fn pause_task(&self, hash: [u8; 20]) -> anyhow::Result<()> {
            anyhow::bail!("RecordingProvider does not support pause_task {}", hex::encode(hash))
        }

        async fn resume_task(&self, hash: [u8; 20]) -> anyhow::Result<()> {
            anyhow::bail!("RecordingProvider does not support resume_task {}", hex::encode(hash))
        }

        async fn delete_task(&self, hash: [u8; 20]) -> anyhow::Result<()> {
            anyhow::bail!("RecordingProvider does not support delete_task {}", hex::encode(hash))
        }
    }

    /// 按路径分量取出，避免测试依赖平台分隔符。
    fn components(path: &Path) -> Vec<PathBuf> {
        path.components()
            .map(|component| PathBuf::from(component.as_os_str()))
            .collect()
    }

    #[tokio::test]
    async fn download_joins_base_path_with_relative_path() {
        let provider = Arc::new(RecordingProvider::default());
        let downloader = Downloader::new(7, "/data/anime".to_string(), provider.clone());
        let hash = [0xABu8; 20];

        let ok = cap::Downloader::download(
            &downloader,
            "magnet:?xt=urn:btih:deadbeef",
            "2024/ep01.mkv",
            hash,
        )
        .await
        .expect("download should not fail");

        assert!(ok);
        let call = provider.last_download();
        assert_eq!(call.url, "magnet:?xt=urn:btih:deadbeef");
        assert_eq!(call.hash, hash);

        let actual = PathBuf::from(&call.path);
        let expected = Path::new("/data/anime").join("2024/ep01.mkv");
        assert!(actual.is_absolute());
        assert_eq!(components(&actual), components(&expected));
    }

    #[tokio::test]
    async fn download_keeps_relative_base_path_relative() {
        let provider = Arc::new(RecordingProvider::default());
        let downloader = Downloader::new(1, "downloads".to_string(), provider.clone());

        cap::Downloader::download(&downloader, "magnet:x", "a/b.mkv", [1u8; 20])
            .await
            .expect("download should not fail");

        let actual = PathBuf::from(&provider.last_download().path);
        let expected = Path::new("downloads").join("a/b.mkv");
        assert!(!actual.is_absolute());
        assert_eq!(components(&actual), components(&expected));
    }

    #[tokio::test]
    async fn download_surfaces_provider_error_as_external_contract_mismatch() {
        let provider = Arc::new(RecordingProvider::failing());
        let downloader = Downloader::new(42, "/data".to_string(), provider.clone());

        let err = cap::Downloader::download(&downloader, "magnet:xyz", "ep01.mkv", [2u8; 20])
            .await
            .expect_err("download should fail when provider fails");

        let expected_path = Path::new("/data").join("ep01.mkv");
        let expected = format!(
            "user 42 use recording download url magnet:xyz to {} failed",
            expected_path.to_string_lossy()
        );
        assert_eq!(err.to_string(), expected);
        assert!(std::error::Error::source(&err).is_some(), "original error chain should be preserved");
    }

    #[test]
    fn provider_returns_the_same_arc() {
        let provider = Arc::new(RecordingProvider::default());
        let downloader = Downloader::new(1, "/data".to_string(), provider.clone());
        let expected: Arc<dyn DownloadProvider> = provider;
        assert!(Arc::ptr_eq(&downloader.provider(), &expected));
    }
}
