use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::entity::{cap::DownloadProvider, model::DownloaderConfig};
use crate::infra::downloader::{qbit::Qbit, rqbit::DefaultDownloader};

pub struct DownloaderManager {
    data_dir: String,
    cache: DashMap<i64, (u64, Arc<dyn DownloadProvider>)>,
}

impl DownloaderManager {
    pub fn new(data_dir: String) -> Self {
        Self {
            data_dir,
            cache: DashMap::new(),
        }
    }

    fn calculate_hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }
}

#[async_trait]
impl crate::entity::cap::DownloaderManager for DownloaderManager {
    async fn get(
        &self,
        user_id: i64,
        config: &DownloaderConfig,
    ) -> Result<Arc<dyn DownloadProvider>> {
        let current_hash = Self::calculate_hash(config);

        if let Some(entry) = self.cache.get(&user_id)
            && entry.0 == current_hash
        {
            return Ok(entry.1.clone());
        }

        let client: Arc<dyn DownloadProvider> = match config {
            DownloaderConfig::Qbit(download_config) => Arc::new(
                Qbit::new(
                    download_config.config.url.clone(),
                    download_config.config.username.clone(),
                    download_config.config.password.clone(),
                )
                .await?,
            ),
            DownloaderConfig::Default(download_config) => {
                let rqbit = DefaultDownloader::new(&download_config.config, &self.data_dir).await?;
                Arc::new(rqbit)
            }
        };

        self.cache.insert(user_id, (current_hash, client.clone()));
        Ok(client)
    }

    async fn validate_config(&self, config: &DownloaderConfig) -> Result<()> {
        match config {
            DownloaderConfig::Qbit(download_config) => {
                let _ = Qbit::new(
                    download_config.config.url.clone(),
                    download_config.config.username.clone(),
                    download_config.config.password.clone(),
                )
                .await?;
                Ok(())
            }
            DownloaderConfig::Default(download_config) => {
                let _ = DefaultDownloader::new(&download_config.config, &self.data_dir).await?;
                Ok(())
            }
        }
    }
}
