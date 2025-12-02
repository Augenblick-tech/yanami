use anyhow::Error;
use reqwest::Client as ReqwestClient;
use rss::Channel;
use std::time::Duration;

pub struct Client {
    client: ReqwestClient,
}

impl Client {
    pub fn new() -> Self {
        let client = ReqwestClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build rss client");
        Client { client }
    }
    pub async fn get_channel(&self, url: &str) -> Result<Channel, Error> {
        let content = self.client.get(url).send().await?.bytes().await?;
        Ok(Channel::read_from(&content[..])?)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
