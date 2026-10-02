use anyhow::Context;
use common::shared::{
    cap::{Downloader, SeriesLocation},
    error::Error,
};

use crate::entity::model::{EpisodeBaseData, EpisodeExtendData, EpsiodeStatus};

#[derive(Clone)]
pub struct EpsiodeEntity {
    data: EpisodeBaseData,
    extend: EpisodeExtendData,
}

impl EpsiodeEntity {
    pub(super) fn new(data: EpisodeBaseData, extend: EpisodeExtendData) -> Self {
        Self { data, extend }
    }

    pub(super) fn get_base_data(&self) -> &EpisodeBaseData {
        &self.data
    }
}

impl EpsiodeEntity {
    pub fn title(&self) -> &str {
        &self.extend.title
    }

    pub fn url(&self) -> &str {
        &self.extend.url
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn ep_num(&self) -> Option<f64> {
        self.data.ep.ep_num
    }

    pub fn sub_anime_id(&self) -> i64 {
        self.data.ep.sub_anime_id
    }

    pub fn anime_id(&self) -> i64 {
        self.extend.anime_id
    }

    pub fn space_id(&self) -> i64 {
        self.extend.space_id
    }

    pub fn resource_id(&self) -> &[u8; 20] {
        &self.data.ep.resource_id
    }

    pub fn status(&self) -> EpsiodeStatus {
        self.data.ep.status.clone()
    }

    pub fn is_downloaded(&self) -> bool {
        self.data.ep.status == EpsiodeStatus::Downloaded
    }

    pub async fn download(
        &mut self,
        downloader: &dyn Downloader,
        series: &dyn SeriesLocation,
    ) -> Result<bool, Error> {
        if let EpsiodeStatus::Downloaded = self.data.ep.status {
            return Ok(true);
        }
        // 落点由系列决定：系列信息或季号取不到就报错，保持 Pending 下次再试，不猜路径
        let path_buf = series.location_of(self.extend.anime_id).await?;
        let path = path_buf
            .to_str()
            .context("build download path failed")
            .map_err(|e| Error::external("download epsiode failed", e))?;
        let res = downloader
            .download(&self.extend.url, path, self.data.ep.resource_id)
            .await?;
        if res {
            self.data.ep.status = EpsiodeStatus::Downloaded;
        }
        Ok(res)
    }

    pub fn reset_download(&mut self) {
        self.data.ep.status = EpsiodeStatus::Pending;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use common::shared::{
        cap::{Downloader, SeriesLocation},
        error::Error,
    };

    use super::EpsiodeEntity;
    use crate::entity::model::{Episode, EpisodeBaseData, EpisodeExtendData, EpsiodeStatus};

    #[derive(Debug, PartialEq, Eq)]
    struct DownloadCall {
        url: String,
        path: String,
        hash: [u8; 20],
    }

    struct RecordingDownloader {
        result: bool,
        calls: Mutex<Vec<DownloadCall>>,
    }

    #[async_trait]
    impl Downloader for RecordingDownloader {
        async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool, Error> {
            self.calls
                .lock()
                .expect("downloader calls lock poisoned")
                .push(DownloadCall {
                    url: url.to_string(),
                    path: path.to_string(),
                    hash,
                });
            Ok(self.result)
        }
    }

    struct RecordingSeries {
        path: PathBuf,
        requested: Mutex<Vec<i64>>,
    }

    #[async_trait]
    impl SeriesLocation for RecordingSeries {
        async fn location_of(&self, anime_id: i64) -> Result<PathBuf, Error> {
            self.requested
                .lock()
                .expect("series requested lock poisoned")
                .push(anime_id);
            Ok(self.path.clone())
        }
    }

    struct FailingSeries {
        requested: Mutex<Vec<i64>>,
    }

    #[async_trait]
    impl SeriesLocation for FailingSeries {
        async fn location_of(&self, anime_id: i64) -> Result<PathBuf, Error> {
            self.requested
                .lock()
                .expect("failing series lock poisoned")
                .push(anime_id);
            Err(Error::external(
                "series lookup failed",
                anyhow::anyhow!("series not found"),
            ))
        }
    }

    fn entity(status: EpsiodeStatus) -> EpsiodeEntity {
        EpsiodeEntity::new(
            EpisodeBaseData {
                id: 5,
                ep: Episode {
                    sub_anime_id: 3,
                    resource_id: [9u8; 20],
                    status,
                    ep_num: Some(1.0),
                },
            },
            EpisodeExtendData {
                title: "某番 第01集".to_string(),
                url: "magnet:?xt=urn:btih:abc".to_string(),
                anime_id: 42,
                space_id: 8,
            },
        )
    }

    #[test]
    fn anime_id_comes_from_extend() {
        assert_eq!(entity(EpsiodeStatus::Pending).anime_id(), 42);
    }

    #[tokio::test]
    async fn download_passes_series_location_and_resource_hash() {
        let mut e = entity(EpsiodeStatus::Pending);
        let downloader = RecordingDownloader {
            result: true,
            calls: Mutex::new(Vec::new()),
        };
        let series = RecordingSeries {
            path: PathBuf::from("/media/library/42/Season 1"),
            requested: Mutex::new(Vec::new()),
        };

        let res = e
            .download(&downloader, &series)
            .await
            .expect("download succeeds");

        assert!(res);
        assert!(e.is_downloaded());
        // 生产代码用 extend.anime_id 询问系列落点
        assert_eq!(
            series.requested.lock().expect("lock").as_slice(),
            &[42]
        );
        let calls = downloader.calls.lock().expect("lock");
        assert_eq!(
            calls.as_slice(),
            &[DownloadCall {
                url: "magnet:?xt=urn:btih:abc".to_string(),
                path: "/media/library/42/Season 1".to_string(),
                hash: [9u8; 20],
            }]
        );
    }

    #[tokio::test]
    async fn download_failure_keeps_episode_pending() {
        let mut e = entity(EpsiodeStatus::Pending);
        let downloader = RecordingDownloader {
            result: false,
            calls: Mutex::new(Vec::new()),
        };
        let series = RecordingSeries {
            path: PathBuf::from("/media/library/42/Season 1"),
            requested: Mutex::new(Vec::new()),
        };

        let res = e
            .download(&downloader, &series)
            .await
            .expect("download returns false without error");

        assert!(!res);
        assert!(!e.is_downloaded());
        assert_eq!(downloader.calls.lock().expect("lock").len(), 1);
    }

    #[tokio::test]
    async fn download_skips_ports_when_already_downloaded() {
        let mut e = entity(EpsiodeStatus::Downloaded);
        let downloader = RecordingDownloader {
            result: true,
            calls: Mutex::new(Vec::new()),
        };
        let series = RecordingSeries {
            path: PathBuf::from("/media/library/42/Season 1"),
            requested: Mutex::new(Vec::new()),
        };

        let res = e.download(&downloader, &series).await.expect("short circuit");

        assert!(res);
        assert!(downloader.calls.lock().expect("lock").is_empty());
        assert!(series.requested.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn download_propagates_series_error_and_keeps_pending() {
        let mut e = entity(EpsiodeStatus::Pending);
        let downloader = RecordingDownloader {
            result: true,
            calls: Mutex::new(Vec::new()),
        };
        let series = FailingSeries {
            requested: Mutex::new(Vec::new()),
        };

        let err = e
            .download(&downloader, &series)
            .await
            .expect_err("series error propagates");

        assert!(err.to_string().contains("series lookup failed"));
        assert!(!e.is_downloaded());
        assert_eq!(series.requested.lock().expect("lock").as_slice(), &[42]);
        // 落点取不到时不调用下载器，保持 Pending 下次再试
        assert!(downloader.calls.lock().expect("lock").is_empty());
    }

    #[test]
    fn accessors_expose_episode_identity_and_reset_download() {
        let mut e = entity(EpsiodeStatus::Downloaded);
        assert_eq!(e.title(), "某番 第01集");
        assert_eq!(e.url(), "magnet:?xt=urn:btih:abc");
        assert_eq!(e.id(), 5);
        assert_eq!(e.ep_num(), Some(1.0));
        assert_eq!(e.sub_anime_id(), 3);
        assert_eq!(e.space_id(), 8);
        assert_eq!(e.resource_id(), &[9u8; 20]);
        assert_eq!(e.get_base_data().id, 5);
        assert_eq!(e.status(), EpsiodeStatus::Downloaded);
        assert!(e.is_downloaded());

        // 重置后回到 Pending，下一次调用可以重新下载
        e.reset_download();
        assert_eq!(e.status(), EpsiodeStatus::Pending);
        assert!(!e.is_downloaded());
    }
}
