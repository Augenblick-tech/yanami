use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use librqbit::{AddTorrent, AddTorrentOptions, Session, SessionOptions, SessionPersistenceConfig};
use librqbit_core::Id20;

use crate::entity::cap::DownloadProvider;
use crate::entity::model::{DefaultDownloaderConfig, DownloadState, DownloadTask};

const DEFAULT_DOWNLOADER_STATE_DIR: &str = "downloader_state";

#[derive(Clone)]
pub struct DefaultDownloader {
    pub session: Arc<Session>,
}

impl DefaultDownloader {
    pub async fn new(_config: &DefaultDownloaderConfig, data_dir: &str) -> Result<Self> {
        let persistence_dir = Path::new(data_dir).join(DEFAULT_DOWNLOADER_STATE_DIR);
        let opts = SessionOptions {
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(persistence_dir.clone()),
            }),
            ..Default::default()
        };

        let session = Session::new_with_opts(persistence_dir, opts)
            .await
            .context("failed to initialize default downloader session")?;

        Ok(Self { session })
    }
}

#[async_trait]
impl DownloadProvider for DefaultDownloader {
    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool> {
        let id20 = Id20::new(hash);

        if self
            .session
            .get(librqbit::api::TorrentIdOrHash::Hash(id20))
            .is_some()
        {
            tracing::debug!("resource {} already in queue, skipping", hex::encode(hash));
            return Ok(true);
        }

        let add_opts = AddTorrentOptions {
            output_folder: Some(path.into()),
            ..Default::default()
        };

        self.session
            .add_torrent(AddTorrent::from_url(url), Some(add_opts))
            .await
            .context("add task failed")?;

        Ok(true)
    }

    fn name(&self) -> &str {
        "default"
    }

    async fn list(&self) -> Result<Vec<DownloadTask>> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let list_resp =
            api.api_torrent_list_ext(librqbit::api::ApiTorrentListOpts { with_stats: true });
        let mut tasks = Vec::new();
        for t in list_resp.torrents {
            let mut state = DownloadState::Error("unknown".to_string());
            let mut progress = 0.0;
            let mut total_size = 0;
            let mut download_speed = 0;

            if let Some(stats) = t.stats {
                state = match stats.state {
                    librqbit::TorrentStatsState::Initializing => DownloadState::Downloading,
                    librqbit::TorrentStatsState::Live => DownloadState::Downloading,
                    librqbit::TorrentStatsState::Paused => DownloadState::Paused,
                    librqbit::TorrentStatsState::Error => {
                        DownloadState::Error(stats.error.unwrap_or_default())
                    }
                };
                if stats.finished {
                    state = DownloadState::Completed;
                }

                total_size = stats.total_bytes;
                progress = if total_size > 0 {
                    stats.progress_bytes as f64 / total_size as f64
                } else {
                    0.0
                };

                if let Some(live) = stats.live {
                    download_speed = (live.download_speed.mbps * 1024.0 * 1024.0) as u64;
                }
            }

            let mut hash = [0u8; 20];
            if let Ok(bytes) = hex::decode(&t.info_hash)
                && bytes.len() == 20
            {
                hash.copy_from_slice(&bytes);
            }

            tasks.push(DownloadTask {
                hash,
                name: t.name.unwrap_or_default(),
                state,
                progress,
                total_size,
                download_speed,
            });
        }
        Ok(tasks)
    }

    async fn get(&self, hash: [u8; 20]) -> Result<Option<DownloadTask>> {
        let tasks = self.list().await?;
        Ok(tasks.into_iter().find(|t| t.hash == hash))
    }

    async fn pause(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_pause(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }

    async fn resume(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_start(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }

    async fn delete(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_delete(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }
}
