use core::fmt;

use anyhow::Error;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct TMDB {
    client: Client,
}

impl fmt::Display for TMDB {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.client)
    }
}

impl TMDB {
    pub fn new(key: &str) -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(format!("Bearer {key}").as_str()).unwrap(),
        );
        headers.insert(header::ACCEPT, "application/json".parse().unwrap());
        Ok(TMDB {
            client: Client::builder().default_headers(headers).build()?,
        })
    }
    pub async fn auth(&mut self) -> Result<(), Error> {
        let url = "https://api.themoviedb.org/3/authentication";
        let rsp = self
            .client
            .get(url)
            .send()
            .await?
            .json::<AuthResult>()
            .await?;
        if rsp.success {
            Ok(())
        } else {
            Err(Error::msg(format!(
                "auth tmdb api key error, {}",
                rsp.status_message
            )))
        }
    }

    pub async fn search(
        &self,
        mode: SearchEnum,
        query: &str,
        mut lang: &str,
    ) -> Result<SearchResult, reqwest::Error> {
        let url = "https://api.themoviedb.org/3/search/";
        if lang.is_empty() {
            lang = "ja";
        }
        self.client
            .get(format!(
                "{}{}?query={}&include_adult=true&language={}",
                url,
                mode.as_str(),
                query,
                lang,
            ))
            .send()
            .await?
            .json::<SearchResult>()
            .await
    }

    pub async fn get_series_details(
        &self,
        series_id: i64,
        mut lang: &str,
    ) -> Result<SeriesResult, Error> {
        if lang.is_empty() {
            lang = "ja";
        }
        let url = format!(
            "https://api.themoviedb.org/3/tv/{}?language={}",
            series_id, lang
        );
        Ok(self.client.get(url).send().await?.json().await?)
    }

    pub async fn get_season_details(
        &self,
        series_id: i64,
        season_id: i64,
    ) -> Result<SeasonResult, reqwest::Error> {
        let url = format!(
            "https://api.themoviedb.org/3/tv/{}/season/{}?language=ja",
            series_id, season_id
        );
        self.client
            .get(url)
            .send()
            .await?
            .json::<SeasonResult>()
            .await
    }
}

#[derive(Debug)]
pub enum SearchEnum {
    Multi,
    TV,
    Movie,
}

impl SearchEnum {
    pub fn as_str(&self) -> &str {
        match self {
            SearchEnum::TV => "tv",
            SearchEnum::Multi => "multi",
            SearchEnum::Movie => "movie",
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct AuthResult {
    success: bool,
    status_code: i64,
    status_message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Results {
    pub adult: bool,
    pub backdrop_path: Option<String>,
    pub id: i64,
    pub name: Option<String>,
    pub original_language: Option<String>,
    pub original_name: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub media_type: Option<String>,
    pub genre_ids: Vec<i64>,
    pub popularity: f64,
    pub first_air_date: Option<String>,
    pub vote_average: f64,
    pub vote_count: f64,
    pub origin_country: Option<Vec<String>>,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub release_date: Option<String>,
    pub video: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchResult {
    pub page: i64,
    pub results: Vec<Results>,
    pub total_pages: i64,
    pub total_results: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesResult {
    pub adult: bool,
    // pub backdrop_path: Option<String>,
    // pub created_by: Vec<CreatedBy>,
    // pub episode_run_time: Vec<i64>,
    // pub first_air_date: Option<String>,
    // pub genres: Vec<Genre>,
    // pub homepage: Option<String>,
    // pub id: i64,
    // pub in_production: bool,
    // pub languages: Vec<String>,
    // pub last_air_date: Option<String>,
    // pub last_episode_to_air: LastEpisodeToAir,
    pub name: Option<String>,
    // pub number_of_episodes: i64,
    // pub number_of_seasons: i64,
    // pub origin_country: Vec<String>,
    // pub original_language: Option<String>,
    // pub original_name: Option<String>,
    // pub overview: Option<String>,
    // pub popularity: f64,
    // pub poster_path: Option<String>,
    // pub production_companies: Vec<ProductionCompany>,
    // pub production_countries: Vec<ProductionCountry>,
    pub seasons: Vec<Season>,
    // pub spoken_languages: Vec<SpokenLanguage>,
    // pub status: Option<String>,
    // pub tagline: Option<String>,
    // #[serde(rename = "type")]
    // pub type_field: Option<String>,
    // pub vote_average: f64,
    // pub vote_count: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedBy {
    pub id: i64,
    pub credit_id: Option<String>,
    pub name: Option<String>,
    pub gender: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Genre {
    pub id: f64,
    pub name: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastEpisodeToAir {
    pub id: i64,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub vote_average: f64,
    pub vote_count: f64,
    pub air_date: Option<String>,
    pub episode_number: i64,
    pub production_code: Option<String>,
    pub runtime: i64,
    pub season_number: i64,
    pub show_id: i64,
    pub still_path: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionCompany {
    pub id: i64,
    pub logo_path: Option<String>,
    pub name: Option<String>,
    pub origin_country: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionCountry {
    pub iso_3166_1: Option<String>,
    pub name: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Season {
    pub air_date: Option<String>,
    pub episode_count: i64,
    pub id: i64,
    pub name: Option<String>,
    // pub overview: Option<String>,
    // pub poster_path: Option<String>,
    pub season_number: i64,
    // pub vote_average: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpokenLanguage {
    pub english_name: Option<String>,
    pub iso_639_1: Option<String>,
    pub name: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonResult {
    // #[serde(rename = "_id")]
    // pub uuid: String,
    // pub air_date: String,
    pub episodes: Vec<Episode>,
    // pub name: String,
    // pub overview: String,
    // #[serde(rename = "id")]
    // pub id: i64,
    // pub poster_path: String,
    // pub season_number: i64,
    // pub vote_average: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    // pub air_date: String,
    pub episode_number: i64,
    // pub id: i64,
    pub name: String,
    // pub overview: String,
    // pub production_code: String,
    // pub runtime: i64,
    pub season_number: i64,
    // pub show_id: i64,
    // pub still_path: Option<String>,
    // pub vote_average: f64,
    // pub vote_count: f64,
}
