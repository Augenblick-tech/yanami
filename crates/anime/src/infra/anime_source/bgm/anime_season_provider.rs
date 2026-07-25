use anyhow::{Error, Result};
use async_trait::async_trait;
use chrono::{Local, NaiveDate};

use crate::{
    entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeMetadata, AnimeSeason,
        AnimeSourceTarget, AnimeTitle,
    },
    infra::anime_source::bgm::{client::BgmClient, model::BangumiItem},
};
#[async_trait]
impl crate::entity::cap::AnimeSeasonalProvider for BgmClient {
    async fn get(&self) -> Result<Vec<AnimeMetadata>> {
        // https://raw.githubusercontent.com/bangumi-data/bangumi-data/refs/heads/master/data/items/2026/07.json
        let today = Local::now().date_naive();
        let (year, month) = Self::season_of_date(&today)?;
        let url = format!(
            "https://raw.githubusercontent.com/bangumi-data/bangumi-data/refs/heads/master/data/items/{}/{:02}.json",
            year, month
        );
        let res = self.client.get(&url).send().await?;
        if res.status() != 200 {
            return Err(Error::msg(format!(
                "bgm get calendar failed, http status code is {}, url is [{}]",
                res.status(),
                url,
            )));
        }
        let items: Vec<BangumiItem> = res.json().await?;
        let mut result = vec![];
        for i in items {
            // 没有官方网站的通常不是日漫，在这个来源大概是脏数据，直接过滤
            if matches!(i.official_site.as_deref(), None | Some("")) {
                tracing::warn!("bgm get calendar check {} not found official_site", i.title);
                continue;
            }

            let mut titles = match i.parse_titles() {
                Some(titles) => titles,
                None => {
                    tracing::error!("bgm get calendar parse {} titles failed", i.title);
                    continue;
                }
            };

            let mut ex_link = match i.parse_ex_link() {
                Ok(ex_link) => ex_link,
                Err(e) => {
                    tracing::error!("bgm get calendar parse {} ex_link failed, {}", i.title, e);
                    continue;
                }
            };

            let id = match ex_link
                .iter()
                .find(|i| i.target == AnimeSourceTarget::Bangumi)
            {
                Some(item) if let AnimeIdType::Int(id) = item.id => id,
                _ => {
                    tracing::error!("bgm get calendar parse {} bgm id failed", i.title);
                    continue;
                }
            };

            let subject = match self.get_subject(id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("bgm get calendar get {} subject failed, {}", i.title, e);
                    continue;
                }
            };

            let mut origin_type = AnimeLangTarget::Other("unknown".to_string());
            let match_name = AnimeTitle::to_keywords(&subject.name)
                .into_iter()
                .collect::<String>();
            // 匹配tmdb主要依靠原名，所以必须先添加一个原名
            titles.push(AnimeTitle {
                name: subject.name.clone(),
                match_name,
                target: origin_type.clone(),
                origin: true,
            });

            if ex_link
                .iter()
                .find(|i| i.target == AnimeSourceTarget::TMDB)
                .is_none()
            {
                let matched = match self.match_tmdb(&titles).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(
                            "bgm get calendar match {} tmdb ex_link failed, {}",
                            i.title,
                            e
                        );
                        continue;
                    }
                };
                ex_link.push(AnimeEx {
                    id: AnimeIdType::Int(matched.inner.id as i64),
                    target: AnimeSourceTarget::TMDB,
                    r#type: Some("tv".to_string()),
                });
            }

            // 检查是否缺少tmdb关联，尝试补全
            let tmdb_id = match ex_link.iter().find(|i| i.target == AnimeSourceTarget::TMDB) {
                Some(tmdb) => {
                    if let AnimeIdType::Int(id) = tmdb.id {
                        id
                    } else {
                        tracing::error!(
                            "bgm get calendar get {} tmdb id from int type failed, got type {:?}",
                            i.title,
                            tmdb.id
                        );
                        continue;
                    }
                }
                None => {
                    let matched = match self.match_tmdb(&titles).await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(
                                "bgm get calendar match {} tmdb ex_link failed, {}",
                                i.title,
                                e
                            );
                            continue;
                        }
                    };
                    ex_link.push(AnimeEx {
                        id: AnimeIdType::Int(matched.inner.id),
                        target: AnimeSourceTarget::TMDB,
                        r#type: Some("tv".to_string()),
                    });
                    matched.inner.id
                }
            };

            let tmdb_data = match self.tmdb.get_tv_detail(tmdb_id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(
                        "bgm get calendar get {} tmdb data by id {} failed, {}",
                        i.title,
                        tmdb_id,
                        e
                    );
                    continue;
                }
            };

            // 补充番剧原名信息
            if let Some(title) = titles.iter_mut().find(|i| i.origin)
                && let AnimeLangTarget::Other(_) = title.target
            {
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

            let eps_num = subject.total_episodes.unwrap_or_else(|| {
                if let Some(eps) = infobox.get("话数")
                    && let Some(eps) = eps.as_u64()
                {
                    return eps as u32;
                }
                0
            });

            let Some(air_weekday) = infobox
                .get("放送星期")
                .and_then(|v| v.as_str())
                .and_then(|v| AnimeAirWeekday::try_from(v).ok())
            else {
                tracing::error!("bgm get calendar parse {} air weekday failed", i.title);
                continue;
            };

            let Some(air_date) = subject
                .date
                .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
                .or_else(|| {
                    infobox
                        .get("放送开始")
                        .and_then(|v| v.as_str())
                        .and_then(|v| NaiveDate::parse_from_str(v, "%Y年%m月%d日").ok())
                })
            else {
                tracing::error!("bgm get calendar parse {} air date failed", i.title);
                continue;
            };

            let season_num = match self.get_anime_season_number(id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("bgm get calendar get {} season failed, {}", i.title, e);
                    continue;
                }
            };
            let mut season = vec![];

            let mut eps = vec![];

            match self.get_anime_eps(id, origin_type.clone()).await {
                Ok(v) => eps = v,
                Err(e) => {
                    tracing::error!("bgm get calendar get {} bgm eps failed, {}", i.title, e);
                }
            }

            season.push(AnimeSeason {
                target: AnimeSourceTarget::Bangumi,
                planned_episode_count: eps_num,
                lang: origin_type.clone(),
                desc: subject.summary.unwrap_or_default(),
                eps,
                season: season_num,
            });

            match self.tmdb.get_anime_season(&tmdb_data, air_date).await {
                Ok(value) => season.push(value),
                Err(e) => {
                    tracing::error!("bgm get calendar get {} tmdb eps failed, {}", i.title, e)
                }
            }

            match self.tmdb.get_anime_titles(tmdb_id).await {
                Ok(value) => {
                    // 因为数据量极小，这里直接循环追加
                    for i in value {
                        if !titles.iter().any(|x| x.name == i.name) {
                            titles.push(i);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("bgm get calendar get {} tmdb titles failed, {}", i.title, e)
                }
            }

            let data = AnimeMetadata {
                external_link: ex_link,
                titles,
                air_weekday,
                air_date,
                air_quarter: (year as u32) * 100 + month,
                season,
            };

            result.push(data);
        }

        Ok(result)
    }
    fn name(&self) -> &str {
        "bgm"
    }
}
