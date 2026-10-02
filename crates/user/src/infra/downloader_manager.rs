use anyhow::{Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::entity::{cap::DownloadProvider, model::DownloaderConfig};
use crate::infra::downloader::{qbit::Qbit, rqbit::DefaultDownloader};

pub struct DownloaderManager {
    data_dir: String,
    cache: DashMap<i64, (u64, Arc<dyn DownloadProvider>)>,
    lock: Mutex<()>,
}

impl DownloaderManager {
    pub fn new(data_dir: String) -> Self {
        Self {
            data_dir,
            cache: DashMap::new(),
            lock: Mutex::new(()),
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

        if let Some((_, (_, provider))) = self.cache.remove(&user_id) {
            provider.stop().await;
        }

        let Ok(_guard) = self.lock.try_lock() else {
            return Err(anyhow!("waiting for init downloader"));
        };
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
            }
            DownloaderConfig::Default(_) => {}
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::cap::DownloaderManager as DownloaderManagerTrait;
    use crate::entity::model::{DefaultDownloaderConfig, DownloadConfig, QbitConfig};

    /// librqbit 的 persistent DHT 会绑定同一默认端口，串行化"会真正创建会话"的用例避免端口冲突。
    static SESSION_LOCK: Mutex<()> = Mutex::const_new(());

    fn default_config(name: &str, max_seed_time: u64) -> DownloaderConfig {
        DownloaderConfig::Default(DownloadConfig {
            name: name.to_string(),
            active: true,
            base_path: "/tmp/yanami-test".to_string(),
            config: DefaultDownloaderConfig {
                max_seed_time: Some(max_seed_time),
                max_seed_ratio: None,
                max_upload_speed: None,
            },
        })
    }

    fn qbit_config(url: &str) -> DownloaderConfig {
        DownloaderConfig::Qbit(DownloadConfig {
            name: "qbit".to_string(),
            active: true,
            base_path: "/data".to_string(),
            config: QbitConfig {
                username: "admin".to_string(),
                password: "pwd".to_string(),
                url: url.to_string(),
            },
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn same_config_reuses_the_cached_provider() {
        let guard = SESSION_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("creating temp dir should not fail");
        let manager = DownloaderManager::new(dir.path().to_string_lossy().to_string());
        let config = default_config("default", 30);

        let first = DownloaderManagerTrait::get(&manager, 1, &config)
            .await
            .expect("first provider fetch should not fail");
        let second = DownloaderManagerTrait::get(&manager, 1, &config)
            .await
            .expect("cache hit should not fail");

        assert_eq!(first.name(), "default");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same config should reuse the same cached provider"
        );

        // librqbit 会从全局 ~/.cache/com.rqbit.dht/dht.json 复用同一个 DHT 端口，
        // 先按生产路径 stop() 释放会话（会中止后台任务并关闭 socket），再放锁。
        first.stop().await;
        drop(first);
        drop(second);
        drop(manager);
        drop(guard);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn different_config_creates_a_different_provider() {
        let guard = SESSION_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("creating temp dir should not fail");
        let manager = DownloaderManager::new(dir.path().to_string_lossy().to_string());

        let first = DownloaderManagerTrait::get(&manager, 1, &default_config("default", 30))
            .await
            .expect("first provider fetch should not fail");
        let second = DownloaderManagerTrait::get(&manager, 1, &default_config("default", 60))
            .await
            .expect("refetching provider after config change should not fail");

        assert!(
            !Arc::ptr_eq(&first, &second),
            "provider should be rebuilt after config hash changes"
        );

        // librqbit 会从全局 ~/.cache/com.rqbit.dht/dht.json 复用同一个 DHT 端口，
        // 先按生产路径 stop() 释放会话（会中止后台任务并关闭 socket），再放锁。
        first.stop().await;
        second.stop().await;
        drop(first);
        drop(second);
        drop(manager);
        drop(guard);
    }

    #[tokio::test]
    async fn validate_config_rejects_invalid_qbit_url_without_network() {
        let manager = DownloaderManager::new("unused".to_string());
        // URL 无法解析，Qbit::new 在 login() 里 Url::parse 时立即失败，不会发起网络请求
        let config = qbit_config("not-a-valid-url");

        let err = DownloaderManagerTrait::validate_config(&manager, &config)
            .await
            .expect_err("invalid qbit url should be rejected");

        assert!(!err.to_string().is_empty(), "error message should not be empty");
        assert!(
            err.to_string().contains("URL"),
            "should report url parse failure, actual: {err}"
        );
    }

    #[tokio::test]
    async fn validate_config_accepts_default_config() {
        let manager = DownloaderManager::new("unused".to_string());

        DownloaderManagerTrait::validate_config(&manager, &default_config("default", 30))
            .await
            .expect("default config validation should pass directly");
    }
}
