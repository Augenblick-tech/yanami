use anyhow::Error;
use reqwest::{
    header::{self, HeaderMap},
    Client,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BgmCalender {
    pub weekday: Weekday,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weekday {
    pub en: String,
    pub cn: String,
    pub ja: String,
    pub id: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    #[serde(rename = "type")]
    pub type_field: i64,
    pub name: String,
    pub name_cn: String,
    pub summary: String,
    pub eps: Option<i64>,
    pub air_date: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalenderAnime {
    pub id: i64,
    pub name: String,
    pub weekday: i64,
    pub eps: i64,
    pub air_date: String,
}

#[derive(Debug)]
pub struct BGM {
    pub client: Client,
}

impl BGM {
    pub fn new() -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, "yaname anna".parse()?);
        Ok(BGM {
            client: Client::builder().default_headers(headers).build()?,
        })
    }

    async fn get_calender(&self) -> Result<Vec<BgmCalender>, Error> {
        let url = "https://api.bgm.tv/calendar";
        Ok(self.client.get(url).send().await?.json().await?)
    }

    async fn get_anime_info(&self, id: i64) -> Result<Option<Item>, Error> {
        let url = format!("https://api.bgm.tv/v0/subjects/{}", id);
        let rsp = self.client.get(url).send().await?;
        if rsp.status() != 200 {
            return Ok(None);
        }
        Ok(Some(rsp.json().await?))
    }

    pub async fn get_calender_anime(&self) -> Result<Vec<CalenderAnime>, Error> {
        let list = self.get_calender().await?;
        let mut calender_anime_list = Vec::new();
        for i in list {
            for item in i.items {
                let info = self
                    .get_anime_info(item.id)
                    .await
                    .expect(format!("get {} info failed", item.id).as_str());
                if info.is_none() {
                    continue;
                }
                let mut info = info.unwrap();
                if info.eps.is_none() || info.eps.unwrap() <= 0 {
                    continue;
                }
                if info.air_date.is_none() {
                    if item.air_date.is_some() {
                        info.air_date = item.air_date;
                    } else {
                        continue;
                    }
                }
                calender_anime_list.push(CalenderAnime {
                    id: info.id,
                    name: info.name,
                    weekday: i.weekday.id,
                    eps: info.eps.unwrap(),
                    air_date: info.air_date.unwrap(),
                });
            }
        }
        Ok(calender_anime_list)
    }
}
