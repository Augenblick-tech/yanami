use crate::{
    entity::{
        cap::AnimeLookupProvider,
        model::{
            AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeMetadata,
            AnimeSearchResult, AnimeSourceTarget,
        },
    },
    infra::anime_source::bgm::client::BgmClient,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::NaiveDate;

#[async_trait]
impl AnimeLookupProvider for BgmClient {
    async fn search(&self, keyword: &str) -> Result<Vec<AnimeSearchResult>> {
        let res = self.search(keyword).await?;
        if let Some(data) = res.data {
            Ok(data
                .into_iter()
                .map(|i| AnimeSearchResult {
                    name: i.name,
                    name_cn: i.name_cn,
                    id: i.id as i64,
                })
                .collect())
        } else {
            Ok(vec![])
        }
    }
    async fn lookup(&self, id: i64) -> Result<Option<AnimeMetadata>> {
        let subject = self.get_subject(id).await?;
        let mut titles = subject.parse_titles();
        let tmdb_data = self.match_tmdb(&titles).await?;
        let ex_link = vec![
            AnimeEx {
                id: AnimeIdType::Int(subject.id as i64),
                target: AnimeSourceTarget::Bangumi,
                r#type: subject.platform.clone(),
            },
            AnimeEx {
                id: AnimeIdType::Int(tmdb_data.inner.id),
                target: AnimeSourceTarget::TMDB,
                r#type: Some("tv".to_string()),
            },
        ];

        let tmdb_data = self.tmdb.get_tv_detail(tmdb_data.inner.id).await?;
        let mut origin_type = AnimeLangTarget::Other("unknown".to_string());
        // 补充番剧原名信息
        if let Some(title) = titles.iter_mut().find(|i| i.origin)
            && let AnimeLangTarget::Other(_) = title.target {
                if let Some(c) = tmdb_data.inner.origin_country.first() {
                    title.target = AnimeLangTarget::from(c.as_str());
                }
                if let AnimeLangTarget::Other(_) = title.target {
                    title.target =
                        AnimeLangTarget::from(tmdb_data.inner.original_language.as_str());
                }
                origin_type = title.target.clone();
            }

        let infobox = subject.parse_infobox();

        let air_weekday = infobox
            .get("放送星期")
            .and_then(|v| v.as_str())
            .and_then(|v| AnimeAirWeekday::try_from(v).ok())
            .context(anyhow!(
                "bgm lookup parse {} air weekday failed",
                &subject.name
            ))?;

        let air_date = subject
            .date
            .clone()
            .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
            .or_else(|| {
                infobox
                    .get("放送开始")
                    .and_then(|v| v.as_str())
                    .and_then(|v| NaiveDate::parse_from_str(v, "%Y年%m月%d日").ok())
            })
            .context(anyhow!(
                "bgm lookup parse {} air date failed",
                &subject.name
            ))?;
        let (year, month) = Self::season_of_date(&air_date)?;

        let tmdb_season = self.tmdb.get_anime_season(&tmdb_data, air_date).await?;

        match self.tmdb.get_anime_titles(tmdb_data.inner.id).await {
            Ok(value) => {
                // 因为数据量极小，这里直接循环追加
                for i in value {
                    if !titles.iter().any(|x| x.name == i.name) {
                        titles.push(i);
                    }
                }
            }
            Err(e) => {
                tracing::error!("bgm lookup get {} tmdb titles failed, {}", &subject.name, e);
            }
        }
        let bgm_season = self
            .get_anime_season(subject, &infobox, origin_type)
            .await?;

        let season = vec![bgm_season, tmdb_season];
        Ok(Some(AnimeMetadata {
            external_link: ex_link,
            titles,
            air_weekday,
            air_date,
            air_quarter: (year as u32) * 100 + month,
            season,
        }))
    }
}
