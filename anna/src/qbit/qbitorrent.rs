use std::collections::HashMap;

use anyhow::Error;
use reqwest::{multipart::Form, Client, ClientBuilder, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::time;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone)]
pub struct Qbit {
    client: Client,
    config: QbitConfig,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams, PartialEq)]
pub struct QbitConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams, PartialEq)]
pub struct QbitState {
    pub torrents: HashMap<String, QbitTorrent>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams, PartialEq)]
pub struct QbitTorrent {
    // pub added_on: i64,
    // pub amount_left: i64,
    // pub auto_tmm: bool,
    // pub availability: i64,
    // pub category: Option<String>,
    // pub completed: i64,
    // pub completion_on: i64,
    // pub content_path: Option<String>,
    // pub dl_limit: i64,
    // pub dlspeed: i64,
    // pub download_path: Option<String>,
    // pub downloaded: i64,
    // pub downloaded_session: i64,
    // pub eta: i64,
    // pub f_l_piece_prio: bool,
    // pub force_start: bool,
    // pub inactive_seeding_time_limit: i64,
    // pub infohash_v1: Option<String>,
    // pub infohash_v2: Option<String>,
    // pub last_activity: i64,
    // pub magnet_uri: Option<String>,
    // pub max_inactive_seeding_time: i64,
    // pub max_ratio: i64,
    // pub max_seeding_time: i64,
    // pub name: Option<String>,
    // pub num_complete: i64,
    // pub num_incomplete: i64,
    // pub num_leechs: i64,
    // pub num_seeds: i64,
    // pub priority: i64,
    // pub progress: i64,
    // pub ratio: f64,
    // pub ratio_limit: i64,
    // pub save_path: Option<String>,
    // pub seeding_time: i64,
    // pub seeding_time_limit: i64,
    // pub seen_complete: i64,
    // pub seq_dl: bool,
    // pub size: i64,
    // pub state: Option<String>,
    // pub super_seeding: bool,
    // pub tags: Option<String>,
    // pub time_active: i64,
    // pub total_size: i64,
    // pub tracker: Option<String>,
    // pub trackers_count: i64,
    // pub up_limit: i64,
    // pub uploaded: i64,
    // pub uploaded_session: i64,
    // pub upspeed: i64,
}

impl Qbit {
    pub fn new(url: String, username: String, password: String) -> Self {
        Qbit {
            client: ClientBuilder::new()
                .cookie_store(true)
                .build()
                .expect("build client"),
            config: QbitConfig {
                url,
                username,
                password,
            },
        }
    }

    pub async fn login(&self) -> Result<(), Error> {
        let url_path = "api/v2/auth/login";
        let url = Url::parse(self.config.url.as_str())?.join(url_path)?;
        let rsp = self
            .client
            .post(url.clone())
            .form(&[
                ("username", self.config.username.to_string()),
                ("password", self.config.password.to_string()),
            ])
            .send()
            .await?;
        if rsp.status() != StatusCode::OK {
            Err(Error::msg(format!(
                "login qbit {} failed, http status code is {}",
                url,
                rsp.status(),
            )))
        } else {
            Ok(())
        }
    }

    pub async fn check_and_login(&self) -> Result<(), Error> {
        let url_path = "api/v2/app/version";
        let rsp = self
            .client
            .get(Url::parse(self.config.url.as_str())?.join(url_path)?)
            .send()
            .await?;
        if rsp.text().await?.contains("Forbidden") {
            self.login().await
        } else {
            Ok(())
        }
    }

    pub async fn add(&self, magnet: &str, save_path: &str, hash: &str) -> Result<(), Error> {
        let rsp = self
            .client
            .post(Url::parse(self.config.url.as_str())?.join("api/v2/torrents/add")?)
            .multipart(
                Form::new()
                    .text("urls", magnet.to_string())
                    .text("autoTMM", "false")
                    .text("savepath", save_path.to_string())
                    .text("paused", "false")
                    .text("stopCondition", "None")
                    .text("contentLayout", "Original")
                    .text("upLimit", "NaN")
                    .text("downLimit", "NaN"),
            )
            .send()
            .await?;
        if rsp.status() != StatusCode::OK {
            return Err(Error::msg(format!(
                "login qbit {} failed, response body is {}",
                self.config.url,
                rsp.text().await?,
            )));
        }
        for _ in 0..5 {
            time::sleep(tokio::time::Duration::from_secs(5)).await;
            if self.check_torrent_in_down_recode(hash).await? {
                return Ok(());
            }
        }
        Err(Error::msg(format!("not check {hash} in down recode")))
    }

    async fn check_torrent_in_down_recode(&self, hash: &str) -> anyhow::Result<bool> {
        let state = self.get_state().await?;
        Ok(state.torrents.contains_key(hash))
    }

    pub async fn get_state(&self) -> anyhow::Result<QbitState> {
        let url_path = "api/v2/sync/maindata";
        let rsp = self
            .client
            .get(Url::parse(self.config.url.as_str())?.join(url_path)?)
            .send()
            .await?;
        if rsp.status() != StatusCode::OK {
            return Err(Error::msg("get qbit state failed"));
        }
        Ok(rsp.json().await?)
    }

    pub async fn load_new_config(&mut self, config: &QbitConfig) -> Result<(), Error> {
        if config.url.is_empty() || config.username.is_empty() || config.password.is_empty() {
            return Ok(());
        }

        if !self.config.eq(config) {
            self.config = config.clone();
            self.login().await?;
        }
        Ok(())
    }
}
