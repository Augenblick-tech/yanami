use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use librqbit::{AddTorrent, AddTorrentOptions, Session, SessionOptions, SessionPersistenceConfig};
use librqbit_core::Id20;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::entity::cap::DownloadProvider;
use crate::entity::model::{DefaultDownloaderConfig, DownloadState, DownloadTask};

const DEFAULT_DOWNLOADER_STATE_DIR: &str = "downloader_state";

pub struct DefaultDownloader {
    pub session: Arc<Session>,
    pub finish_times: Arc<dashmap::DashMap<Id20, std::time::Instant>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl DefaultDownloader {
    pub async fn new(config: &DefaultDownloaderConfig, data_dir: &str) -> Result<Self> {
        let persistence_dir = Path::new(data_dir).join(DEFAULT_DOWNLOADER_STATE_DIR);

        let mut limits = librqbit::limits::LimitsConfig::default();
        if let Some(speed) = config.max_upload_speed {
            let bps = speed.saturating_mul(1024);
            if let Ok(nz) = u32::try_from(bps) {
                limits.upload_bps = std::num::NonZeroU32::new(nz);
            }
        }

        let opts = SessionOptions {
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(persistence_dir.clone()),
            }),
            listen_port_range: Some(6881..6889),
            enable_upnp_port_forwarding: true,
            ratelimits: limits,
            ..Default::default()
        };

        let session = Session::new_with_opts(persistence_dir, opts)
            .await
            .context("failed to initialize default downloader session")?;

        let max_seed_time = config.max_seed_time.map(|m| Duration::from_secs(m * 60));
        let max_seed_ratio = config.max_seed_ratio;
        let finish_times = Arc::new(dashmap::DashMap::new());

        let worker = Some(Self::start_seed_monitor(
            session.clone(),
            finish_times.clone(),
            max_seed_time,
            max_seed_ratio,
        ));

        Ok(Self {
            session,
            finish_times,
            worker: Mutex::new(worker),
        })
    }

    fn start_seed_monitor(
        session: Arc<Session>,
        finish_times: Arc<dashmap::DashMap<Id20, std::time::Instant>>,
        max_seed_time: Option<Duration>,
        max_seed_ratio: Option<f64>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let api = librqbit::api::Api::new(session, None);

            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                let list_resp = api
                    .api_torrent_list_ext(librqbit::api::ApiTorrentListOpts { with_stats: true });

                let mut current_ids = std::collections::HashSet::new();

                for t in list_resp.torrents {
                    let mut hash = [0u8; 20];
                    if let Ok(bytes) = hex::decode(&t.info_hash) {
                        if bytes.len() == 20 {
                            hash.copy_from_slice(&bytes);
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    let id20 = Id20::new(hash);
                    current_ids.insert(id20);

                    if let Some(stats) = t.stats {
                        if stats.finished
                            && matches!(stats.state, librqbit::TorrentStatsState::Live)
                        {
                            let finish_time = *finish_times
                                .entry(id20)
                                .or_insert_with(std::time::Instant::now);
                            let mut should_pause = false;

                            if let Some(limit_time) = max_seed_time
                                && finish_time.elapsed() >= limit_time
                            {
                                should_pause = true;
                            }

                            if let Some(limit_ratio) = max_seed_ratio
                                && stats.total_bytes > 0
                            {
                                let ratio = stats.uploaded_bytes as f64 / stats.total_bytes as f64;
                                if ratio >= limit_ratio {
                                    should_pause = true;
                                }
                            }

                            if should_pause
                                && let Err(e) = api
                                    .api_torrent_action_pause(librqbit::api::TorrentIdOrHash::Hash(
                                        id20,
                                    ))
                                    .await
                            {
                                tracing::warn!("failed to pause torrent: {:?}", e);
                            }
                        } else if !stats.finished {
                            finish_times.remove(&id20);
                        }
                    }
                }

                finish_times.retain(|id, _| current_ids.contains(id));
            }
        })
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

    async fn stop(&self) {
        if let Ok(mut worker) = self.worker.try_lock() {
            if let Some(worker) = worker.take() {
                worker.abort();
            }
            self.session.stop().await;
        }
    }

    async fn list_task(&self) -> Result<Vec<DownloadTask>> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let list_resp =
            api.api_torrent_list_ext(librqbit::api::ApiTorrentListOpts { with_stats: true });
        let mut tasks = Vec::new();
        for t in list_resp.torrents {
            let mut state = DownloadState::Error("unknown".to_string());
            let mut progress = 0.0;
            let mut total_size = 0;
            let mut download_speed = 0;

            let mut is_seeding = false;
            let mut upload_speed = 0;
            let mut seed_ratio = 0.0;
            let mut seed_duration = None;

            let mut hash = [0u8; 20];
            if let Ok(bytes) = hex::decode(&t.info_hash)
                && bytes.len() == 20
            {
                hash.copy_from_slice(&bytes);
            }

            let id20 = Id20::new(hash);

            if let Some(stats) = &t.stats {
                state = match stats.state {
                    librqbit::TorrentStatsState::Initializing => DownloadState::Downloading,
                    librqbit::TorrentStatsState::Live => DownloadState::Downloading,
                    librqbit::TorrentStatsState::Paused => DownloadState::Paused,
                    librqbit::TorrentStatsState::Error => {
                        DownloadState::Error(stats.error.clone().unwrap_or_default())
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

                if stats.total_bytes > 0 {
                    seed_ratio = stats.uploaded_bytes as f64 / stats.total_bytes as f64;
                }

                if stats.finished && matches!(stats.state, librqbit::TorrentStatsState::Live) {
                    is_seeding = true;
                    if let Some(time) = self.finish_times.get(&id20) {
                        seed_duration = Some(time.elapsed().as_secs());
                    }
                }

                if let Some(live) = &stats.live {
                    download_speed = (live.download_speed.mbps * 1024.0 * 1024.0) as u64;
                    upload_speed = (live.upload_speed.mbps * 1024.0 * 1024.0) as u64;
                }
            }

            tasks.push(DownloadTask {
                hash,
                name: t.name.unwrap_or_default(),
                state,
                progress,
                total_size,
                download_speed,
                is_seeding,
                upload_speed,
                seed_ratio,
                seed_duration,
            });
        }
        Ok(tasks)
    }

    async fn get_task(&self, hash: [u8; 20]) -> Result<Option<DownloadTask>> {
        let tasks = self.list_task().await?;
        Ok(tasks.into_iter().find(|t| t.hash == hash))
    }

    async fn pause_task(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_pause(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }

    async fn resume_task(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_start(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }

    async fn delete_task(&self, hash: [u8; 20]) -> Result<()> {
        let api = librqbit::api::Api::new(self.session.clone(), None);
        let id20 = Id20::new(hash);
        api.api_torrent_action_delete(librqbit::api::TorrentIdOrHash::Hash(id20))
            .await?;
        Ok(())
    }
}
