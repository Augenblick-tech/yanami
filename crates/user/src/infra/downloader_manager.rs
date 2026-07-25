use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::{DashMap, Entry};
use reqwest::Client;

use crate::entity::{cap::DownloadProvider, model::DownloaderConfig};
use crate::infra::downloader::qbit::Qbit;

#[derive(Clone)]
pub struct DownloaderManager {
    // TODO: Qbit需要使用独立的cookie，所以不复用全局客户端，后续需要使用时再用
    _client: Client,
    cache: DashMap<i64, (u64, Arc<dyn DownloadProvider>)>,
}

impl DownloaderManager {
    pub fn new(client: Client) -> Self {
        Self {
            _client: client,
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
        let client = match self.cache.entry(user_id) {
            Entry::Occupied(mut entry) => {
                if entry.get().0 == current_hash {
                    entry.get().1.clone()
                } else {
                    match config {
                        DownloaderConfig::Qbit(download_config) => {
                            let client = Arc::new(
                                Qbit::new(
                                    download_config.config.url.clone(),
                                    download_config.config.username.clone(),
                                    download_config.config.password.clone(),
                                )
                                .await?,
                            );
                            entry.insert((current_hash, client.clone()));
                            client
                        }
                    }
                }
            }
            Entry::Vacant(entry) => match config {
                DownloaderConfig::Qbit(download_config) => {
                    let client = Arc::new(
                        Qbit::new(
                            download_config.config.url.clone(),
                            download_config.config.username.clone(),
                            download_config.config.password.clone(),
                        )
                        .await?,
                    );
                    entry.insert((current_hash, client.clone()));
                    client
                }
            },
        };
        Ok(client)
    }

    async fn validate_config(&self, config: &DownloaderConfig) -> Result<()> {
        match config {
            DownloaderConfig::Qbit(download_config) => {
                // Qbit::new automatically attempts a login, serving as a connection test
                let _ = Qbit::new(
                    download_config.config.url.clone(),
                    download_config.config.username.clone(),
                    download_config.config.password.clone(),
                )
                .await?;
                Ok(())
            }
        }
    }
}
