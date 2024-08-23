use anyhow::Error;
use reqwest::{multipart::Form, Client, ClientBuilder, StatusCode, Url};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone)]
pub struct Qbit {
    client: Client,
    config: QbitConfig,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct QbitConfig {
    pub url: String,
    pub username: String,
    pub password: String,
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
        let url_path = "/api/v2/auth/login";
        let rsp = self
            .client
            .post(Url::parse(self.config.url.as_str())?.join(url_path)?)
            .form(&[
                ("username", self.config.username.to_string()),
                ("password", self.config.password.to_string()),
            ])
            .send()
            .await?;
        if rsp.status() != StatusCode::OK {
            Err(Error::msg(format!(
                "login qbit {} failed, http status code is {}",
                self.config.url,
                rsp.status()
            )))
        } else {
            Ok(())
        }
    }

    pub async fn check_and_login(&self) -> Result<(), Error> {
        let url_path = "/api/v2/app/version";
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

    pub async fn add(&self, magnet: &str, save_path: &str) -> Result<(), Error> {
        let rsp = self
            .client
            .post(Url::parse(self.config.url.as_str())?.join("/api/v2/torrents/add")?)
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
            Err(Error::msg(format!(
                "login qbit {} failed, http status code is {}, response body is {}",
                self.config.url,
                rsp.status(),
                rsp.text().await?,
            )))
        } else {
            Ok(())
        }
    }
}
