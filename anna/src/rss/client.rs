use anyhow::Error;
use rss::Channel;

pub struct Client {
    pub client: reqwest::Client,
}

impl Client {
    pub fn new() -> Self {
        Client {
            client: reqwest::Client::new(),
        }
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
