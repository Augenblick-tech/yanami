use std::collections::HashSet;

use anyhow::{Context, Error};
use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    bgm::bangumi::BGM,
    tmdb::client::{SearchEnum, TMDB},
};
use utoipa::ToSchema;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Eq)]
pub struct AnimeInfo {
    pub id: i64,
    pub name: String,
    pub weekday: i64,
    pub eps: i64,
    pub air_date: String,
    pub name_tw: String,
    pub name_cn: String,
    pub season: i64,
    pub search_name: String,
    pub alternative_titles: Option<Vec<String>>,
}

impl AnimeInfo {
    pub fn names(&self) -> Vec<String> {
        if let Some(titles) = self.alternative_titles.clone() {
            titles
        } else {
            [&self.name, &self.name_tw, &self.name_cn]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
    }
}

pub struct AnimeTracker {
    tmdb: TMDB,
    bgm: BGM,
}

impl AnimeTracker {
    pub fn new(tmdb: TMDB, bgm: BGM) -> Self {
        AnimeTracker { tmdb, bgm }
    }

    pub async fn get_calender(&self) -> Result<Vec<AnimeInfo>, Error> {
        let mut anime_info_list = Vec::new();
        let rsp = self
            .bgm
            .get_calender_anime()
            .await
            .context("get calender failed")?;
        let re = Regex::new("第[0-9]+期").context("set re rule failed")?;
        let en_re = Regex::new("[Ss]eason.*?$").context("set re rule failed")?;
        let en_nd_re = Regex::new(r"\d+.*?[Ss]eason.*?$").context("set re rule failed")?;
        let end_number_re = Regex::new(r"\d+$").context("set re rule failed")?;
        for bgm in rsp.iter() {
            let name = re.replace(&bgm.name, "").trim().to_string();
            let name = en_nd_re.replace(&name, "").trim().to_string();
            let mut name = en_re.replace(&name, "").trim().to_string();
            let mut search_result = self
                .tmdb
                .search(SearchEnum::TV, &name, "zh-TW")
                .await
                .context("search failed")?;
            if search_result.results.is_empty() {
                name = end_number_re.replace(&name, "").trim().to_string();
                search_result = self
                    .tmdb
                    .search(SearchEnum::TV, &name, "zh-TW")
                    .await
                    .context("search failed")?;
                if search_result.results.is_empty() {
                    // println!(
                    //     "search empty skip, name:{}, search name: {}",
                    //     &bgm.name, name
                    // );
                    continue;
                }
            }
            let mut res = Option::None;
            if let Ok(bgm_date) = NaiveDate::parse_from_str(&bgm.air_date, "%Y-%m-%d") {
                for i in search_result.results.iter() {
                    if let Some(air_date) = &i.first_air_date {
                        if let Ok(air_date) = NaiveDate::parse_from_str(air_date, "%Y-%m-%d") {
                            if air_date.year() == bgm_date.year()
                                && air_date.month() == bgm_date.month()
                            {
                                res = Some(i.clone());
                                break;
                            }
                        }
                    }
                }
            }
            if res.is_none() {
                res = Some(search_result.results.first().unwrap().clone());
            }
            if res.is_none() {
                continue;
            }
            let res = res.unwrap();
            if !res
                .original_language
                .clone()
                .context("not found original_language")?
                .eq("ja")
            {
                // println!(
                //     "not found jp anime\nres: {:?}\nbgm: {:?},search name: {}",
                //     &res, &bgm, name
                // );
                continue;
            }
            let series_result = self
                .tmdb
                .get_series_details(res.id, "zh-CN")
                .await
                .context("get series failed")?;
            let season = series_result.seasons.last().context("not found season")?;
            if season.season_number <= 0 {
                continue;
            }
            if bgm.eps <= 0 && season.episode_count <= 0 {
                continue;
            }

            let alternative_titles = self.tmdb.get_alternative_titles(res.id).await?;
            let mut names = alternative_titles
                .results
                .iter()
                .map(|v| v.title.clone())
                .collect::<HashSet<String>>();
            names.insert(bgm.name.clone());
            names.insert(series_result.name.clone().context("not found name cn")?);
            names.insert(res.name.clone().context("not found name tw")?);

            let anime_info = AnimeInfo {
                id: bgm.id,
                name: bgm.name.clone(),
                name_cn: series_result.name.clone().context("not found name cn")?,
                name_tw: res.name.context("not found name tw")?,
                weekday: bgm.weekday,
                air_date: bgm.air_date.clone(),
                eps: if bgm.eps > 0 {
                    bgm.eps
                } else {
                    season.episode_count
                },
                season: season.season_number,
                search_name: name,
                alternative_titles: Some(names.into_iter().collect()),
            };
            anime_info_list.push(anime_info);
        }
        Ok(anime_info_list)
    }
}
