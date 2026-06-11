use std::path::PathBuf;

use anyhow::Context;
use common::shared::{cap::Downloader, error::Error};

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

    pub async fn download(&mut self, downloader: &dyn Downloader) -> Result<bool, Error> {
        if let EpsiodeStatus::Downloaded = self.data.ep.status {
            return Ok(true);
        }
        let path_buf = self.build_download_path();
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
}

impl EpsiodeEntity {
    fn build_download_path(&self) -> PathBuf {
        let anime_name = self
            .extend
            .anime_origin_title
            .replace(&['/', '\\', ':', '*', '?', '"', '<', '>', '|'][..], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let season_dir = format!("S{:02}", self.extend.season);
        PathBuf::from(anime_name).join(season_dir)
    }
}
