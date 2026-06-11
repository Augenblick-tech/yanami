use crate::infra::anime_source::tmdb::{
    client::TmdbClient,
    model::{Page, SearchTVResult, SeriesAlternativeTitles, TvSeasonDetail, TvShowDetail},
};
use anyhow::Result;

impl TmdbClient {
    pub async fn search_tv(&self, keyword: &str) -> Result<Page<SearchTVResult>> {
        let url = format!(
            "https://api.themoviedb.org/3/search/tv?query={}&include_adult=true&language=zh-CN",
            keyword
        );
        let res = self.get::<Page<SearchTVResult>>(&url).await?;
        Ok(res)
    }

    pub async fn get_tv_alternative_titles(&self, id: i64) -> Result<SeriesAlternativeTitles> {
        let url = format!("https://api.themoviedb.org/3/tv/{}/alternative_titles", id);
        let res = self.get::<SeriesAlternativeTitles>(&url).await?;
        Ok(res)
    }

    pub async fn get_tv_detail(&self, id: i64) -> Result<TvShowDetail> {
        let url = format!("https://api.themoviedb.org/3/tv/{}?language=zh-CN", id);
        self.get(&url).await
    }

    pub async fn get_tv_season_eps(&self, tv_id: i64, season_id: i64) -> Result<TvSeasonDetail> {
        let url = format!(
            "https://api.themoviedb.org/3/tv/{}/season/{}?language=zh-CN",
            tv_id, season_id
        );
        self.get(&url).await
    }
}
