use anyhow::Error;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    bgm::bgm::BGM,
    tmdb::tmdb::{SearchEnum, TMDB},
};
use utoipa::ToSchema;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AnimeInfo {
    pub id: i64,
    pub name: String,
    pub weekday: i64,
    pub eps: i64,
    pub air_date: String,
    pub name_tw: String,
    pub name_cn: String,
    pub season: i64,
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
            .expect("get calender failed");
        let re = Regex::new("第[0-9]+期").expect("set re rule failed");
        let en_re = Regex::new("Season.*?$").expect("set re rule failed");
        for bgm in rsp.iter() {
            let name = re.replace(&bgm.name, "").trim().to_string();
            let name = en_re.replace(&name, "").trim().to_string();
            let search_result = self
                .tmdb
                .search(SearchEnum::TV, &name, "zh-TW")
                .await
                .expect("search failed");
            if search_result.results.is_empty() {
                // println!(
                //     "search empty skip, name:{}, search name: {}",
                //     &bgm.name, name
                // );
                continue;
            }
            let res = search_result.results.get(0).unwrap().clone();
            if !res
                .original_language
                .clone()
                .expect("not found original_language")
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
                .expect("get series failed");
            let season = series_result.seasons.last().expect("not found season");
            if season.season_number <= 0 {
                continue;
            }
            let anime_info = AnimeInfo {
                id: bgm.id,
                name: bgm.name.clone(),
                name_cn: series_result.name.clone().expect("not found name cn"),
                name_tw: res.name.expect("not found name tw"),
                weekday: bgm.weekday,
                air_date: season.air_date.clone().unwrap(),
                eps: bgm.eps,
                season: season.season_number,
            };
            anime_info_list.push(anime_info);
        }
        Ok(anime_info_list)
    }
}
