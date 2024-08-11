use anyhow::Error;
use reqwest::Client;
use rss::Channel;

pub struct RssHttpClient {
    pub client: Client,
}

impl RssHttpClient {
    pub fn new() -> Self {
        RssHttpClient {
            client: reqwest::Client::new(),
        }
    }
    pub async fn get_channel(&self, url: &str) -> Result<Channel, Error> {
        let content = self.client.get(url).send().await?.bytes().await?;
        Ok(Channel::read_from(&content[..])?)
    }
}
