use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::vec;

use crate::{
    entity::model::{
        AnimeEpisode, AnimeIdType, AnimeLangTarget, AnimeSeason, AnimeSourceTarget, AnimeTitle,
    },
    infra::anime_source::tmdb::model::TvShowDetail,
};

#[derive(Clone)]
pub struct TmdbClient {
    // client: Arc<tmdb_api::Client<ReqwestExecutor>>,
    pub(super) http_client: Client,
    pub(super) token: String,
}

impl TmdbClient {
    pub fn new(token: &str, http_client: Client) -> Self {
        // let client = tmdb_api::Client::<ReqwestExecutor>::new(token.into());
        Self {
            // client: Arc::new(client),
            http_client,
            token: format!("Bearer {}", token),
        }
    }
}

impl TmdbClient {
    pub(super) async fn get<T>(&self, url: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let res = self
            .http_client
            .get(url)
            .header("accept", "application/json")
            .header("Authorization", self.token.clone())
            .send()
            .await?;
        if res.status() != 200 {
            return Err(anyhow!(
                "tmdb http client get fetch {} failed, http status code is {}",
                url,
                res.status()
            ));
        }
        Ok(res.json().await?)
    }
}

impl TmdbClient {
    pub async fn get_anime_titles(&self, id: i64) -> Result<Vec<AnimeTitle>> {
        let res = self.get_tv_alternative_titles(id).await?;
        let titles = res
            .results
            .into_iter()
            .map(|i| AnimeTitle {
                match_name: AnimeTitle::to_keywords(&i.title)
                    .into_iter()
                    .collect::<String>(),
                name: i.title,
                target: AnimeLangTarget::from(i.iso_3166_1.as_str()),
                origin: false,
            })
            .collect();
        Ok(titles)
    }

    // get_season_id
    // 匹配时间最接近的季度
    pub fn get_season_id(&self, series: &TvShowDetail, air_date: NaiveDate) -> Option<i64> {
        let dates = series
            .seasons
            .iter()
            .filter_map(|i| i.air_date)
            .collect::<Vec<_>>();
        let date = find_closest_date(&dates, air_date);
        if let Some(date) = date
            && let Some(v) = series.seasons.iter().find(|i| i.air_date == Some(date)) {
                return Some(v.inner.season_number);
            }
        None
    }

    pub async fn get_anime_season(
        &self,
        series: &TvShowDetail,
        air_date: NaiveDate,
    ) -> Result<AnimeSeason> {
        let season_id = self
            .get_season_id(series, air_date)
            .context("not found tmdb season")?;
        let season_data = self.get_tv_season_eps(series.inner.id, season_id).await?;
        let eps = season_data
            .episodes
            .iter()
            .filter(|i| i.inner.air_date.is_some())
            .map(|i| {
                AnimeEpisode {
                    ep: i.inner.episode_number as u32,
                    sort: i.inner.episode_number as f64,
                    air_date: i.inner.air_date.unwrap(),
                    title: vec![AnimeTitle {
                        name: i.inner.name.clone(),
                        match_name: AnimeTitle::to_keywords(&i.inner.name)
                            .into_iter()
                            .collect::<String>(),
                        target: AnimeLangTarget::ZhCn,
                        origin: false,
                    }],
                    // tmdb-api 这个库缺少了runtime字段，拿不到时长
                    duration_seconds: 0,
                    desc: i.inner.overview.clone().unwrap_or_default(),
                    ex_id: AnimeIdType::Int(i.inner.id),
                }
            })
            .collect::<Vec<_>>();
        let season = AnimeSeason {
            target: AnimeSourceTarget::TMDB,
            lang: AnimeLangTarget::ZhCn,
            desc: season_data.inner.overview.unwrap_or_default(),
            season: season_id as u32,
            eps,
            planned_episode_count: if season_data.inner.season_number
                > season_data.episodes.len() as i64
            {
                season_data.inner.season_number as u32
            } else {
                season_data.episodes.len() as u32
            },
        };
        Ok(season)
    }
}

impl TmdbClient {
    // pub async fn search_tv(&self, keyword: &str) -> Result<PaginatedResult<TVShowShort>> {
    //     let cmd = TVShowSearch::new(keyword.into()).with_language(Some("zh-CN".to_string()));
    //     let res = cmd
    //         .execute(&self.client)
    //         .await
    //         .map_err(|e| anyhow::Error::msg(e.to_string()))?;
    //     Ok(res)
    // }

    // pub async fn get_tv_detail(&self, id: u64) -> Result<TVShow> {
    //     let res = TVShowDetails::new(id)
    //         .with_language(Some("zh-CN".to_string()))
    //         .execute(&self.client)
    //         .await
    //         .map_err(|e| Error::msg(e.to_string()))?;
    //     Ok(res)
    // }

    // pub async fn get_tv_season_eps(&self, series_id: u64, season_id: u64) -> Result<Season> {
    //     let res = TVShowSeasonDetails::new(series_id, season_id)
    //         .with_language(Some("zh-CN".to_string()))
    //         .execute(&self.client)
    //         .await
    //         .map_err(|e| anyhow!("{}, series_id={}, season_id={}", e, series_id, season_id))?;
    //     Ok(res)
    // }
}

/// 从日期列表中找出与目标日期最接近的日期。
/// 如果有两个日期距离相同，则返回较早的那个。
fn find_closest_date(dates: &[NaiveDate], target: NaiveDate) -> Option<NaiveDate> {
    if dates.is_empty() {
        return None;
    }

    let mut closest = dates[0];
    // 使用 i64 来存储天数差，避免溢出
    let mut min_diff = (target - closest).num_days().abs();

    for &date in &dates[1..] {
        let diff = (target - date).num_days().abs();
        if diff < min_diff {
            min_diff = diff;
            closest = date;
        }
    }

    Some(closest)
}
