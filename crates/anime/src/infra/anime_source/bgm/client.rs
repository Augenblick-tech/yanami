use std::collections::HashMap;

use anyhow::{Context, Error, Result, anyhow};
use chrono::{Datelike, NaiveDate};
use reqwest::Client;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::{
    entity::model::{
        AnimeEpisode, AnimeIdType, AnimeLangTarget, AnimeSeason, AnimeSourceTarget, AnimeTitle,
    },
    infra::anime_source::{
        bgm::model::{
            BangumiSubject, Episode, FilterConfig, Page, RelatedSubject, Relation, SearchQuery,
            SubjectType,
        },
        tmdb::{client::TmdbClient, model::SearchTVResult},
    },
};

#[derive(Clone)]
pub struct BgmClient {
    pub(super) client: Client,
    pub(super) tmdb: TmdbClient,
}

impl BgmClient {
    pub fn new(http_client: Client, tmdb: TmdbClient) -> Self {
        Self {
            client: http_client,
            tmdb,
        }
    }
}

impl BgmClient {
    // get_anime_season
    // 循环请求 get_subjects 计算当前季度是第几季
    pub(super) async fn get_anime_season_number(&self, mut id: i64) -> Result<u32> {
        let mut season_number = 1_u32;
        loop {
            if season_number > 50 {
                return Err(Error::msg(
                    "bgm get anime season failed, loop too many times",
                ));
            }

            let subs = self.get_subjects(id).await?;
            let Some(prequel) = subs
                .iter()
                .find(|&i| i.subject_type == SubjectType::Anime && i.relation == Relation::Prequel)
            else {
                return Ok(season_number);
            };
            id = prequel.id as i64;

            let detail = self.get_subject(id).await?;
            if detail
                .platform
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                == "tv"
            {
                season_number += 1;
            }
        }
    }

    pub(super) async fn match_tmdb(&self, titles: &[AnimeTitle]) -> Result<SearchTVResult> {
        let names = titles
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<&str>>();
        let original_title = titles
            .iter()
            .find(|i| i.origin)
            .context("not found origin title")?;

        let mut error = None;
        let keywords = to_keywords(&names)?;
        let mut max_score = 0.0;
        let mut target = None;
        for key in keywords {
            let res = self.tmdb.search_tv(&key).await;
            let res = match res {
                Ok(i) => i,
                Err(e) => {
                    error = Some(e);
                    continue;
                }
            };
            if res.results.is_empty() {
                continue;
            }

            for item in res.results {
                // 匹配原名
                let mut score = is_str_match(&original_title.name, &item.inner.original_name);
                // 如果原名不是简中时，因为TMDB使用的搜索语言是简中，尝试匹配简中译名
                if original_title.target != AnimeLangTarget::ZhCn {
                    // 匹配简中翻译
                    let cn_score = titles
                        .iter()
                        .filter(|t| t.target == AnimeLangTarget::ZhCn)
                        .map(|t| is_str_match(&t.name, &item.inner.name))
                        .fold(0.0_f64, f64::max);
                    score = score.max(cn_score);
                }
                if max_score < score {
                    max_score = score;
                    target = Some(item);
                }
            }
        }

        if let Some(target) = target
            && max_score > 0.4
        {
            tracing::debug!(
                "matched {}, original_name {}, score {}",
                &target.inner.original_name,
                &original_title.name,
                max_score
            );
            return Ok(target);
        }

        if let Some(error) = error {
            Err(error)
        } else {
            Err(anyhow!("not found {} in tmdb", &original_title.name))
        }
    }

    pub async fn get_anime_eps(
        &self,
        id: i64,
        origin: AnimeLangTarget,
    ) -> Result<Vec<AnimeEpisode>> {
        let res = self.get_epsiodes(id).await?;
        let mut eps = vec![];
        if res.total == 0 {
            return Ok(eps);
        }
        if let Some(data) = res.data {
            for i in data {
                let mut titles = vec![];
                if matches!(i.name.as_deref(), None | Some("")) {
                    continue;
                }
                let name = i.name.clone().context("not found ep origin title")?;
                let match_name = AnimeTitle::to_keywords(&name)
                    .into_iter()
                    .collect::<String>();
                titles.push(AnimeTitle {
                    name,
                    match_name,
                    target: origin.clone(),
                    origin: true,
                });

                if !matches!(i.name_cn.as_deref(), None | Some("")) {
                    let name_cn = i.name_cn.clone().context("not found ep cn title")?;
                    let match_name = AnimeTitle::to_keywords(&name_cn)
                        .into_iter()
                        .collect::<String>();
                    titles.push(AnimeTitle {
                        name: name_cn,
                        match_name,
                        target: AnimeLangTarget::ZhCn,
                        origin: false,
                    });
                }

                let ep = AnimeEpisode {
                    ep: i.ep,
                    sort: i.sort,
                    air_date: NaiveDate::parse_from_str(&i.airdate, "%Y-%m-%d")?,
                    title: titles,
                    duration_seconds: i.duration_seconds as u64,
                    desc: i.desc.context("not found ep desc")?,
                    ex_id: AnimeIdType::Int(i.id as i64),
                };
                eps.push(ep);
            }
        }
        Ok(eps)
    }

    pub(super) fn season_of_date(date: &NaiveDate) -> Result<(i32, u32)> {
        match date.month() {
            1..=3 => Ok((date.year(), 1)),
            4..=6 => Ok((date.year(), 4)),
            7..=9 => Ok((date.year(), 7)),
            10..=12 => Ok((date.year(), 10)),
            _ => Err(anyhow!("month must be 1..=12")),
        }
    }

    pub(super) async fn get_anime_season(
        &self,
        subject: BangumiSubject,
        infobox: &HashMap<String, Value>,
        origin_type: AnimeLangTarget,
    ) -> Result<AnimeSeason> {
        let mut eps = vec![];
        match self
            .get_anime_eps(subject.id as i64, origin_type.clone())
            .await
        {
            Ok(v) => eps = v,
            Err(e) => {
                tracing::error!(
                    "bgm get calendar get {} bgm eps failed, {}",
                    subject.name,
                    e
                );
            }
        }

        let eps_num = subject.total_episodes.unwrap_or_else(|| {
            if let Some(eps) = infobox.get("话数")
                && let Some(eps) = eps.as_u64()
            {
                return eps as u32;
            }
            0
        });

        let season_num = self.get_anime_season_number(subject.id as i64).await?;

        Ok(AnimeSeason {
            target: AnimeSourceTarget::Bangumi,
            planned_episode_count: eps_num,
            lang: origin_type,
            desc: subject.summary.unwrap_or_default(),
            eps,
            season: season_num,
        })
    }
}

// bgm api 实现
impl BgmClient {
    pub async fn get_subject(&self, id: i64) -> Result<BangumiSubject> {
        let res = self
            .client
            .get(format!("https://api.bgm.tv/v0/subjects/{}", id))
            .send()
            .await?;
        if res.status() != 200 {
            return Err(Error::msg(format!(
                "bgm get subject failed, http status code is {}",
                res.status()
            )));
        }
        let subject = res.json::<BangumiSubject>().await?;
        Ok(subject)
    }

    pub async fn get_subjects(&self, id: i64) -> Result<Vec<RelatedSubject>> {
        let res = self
            .client
            .get(format!("https://api.bgm.tv/v0/subjects/{}/subjects", id))
            .send()
            .await?;
        if res.status() != 200 {
            return Err(Error::msg(format!(
                "bgm get subjects failed, http status code is {}",
                res.status()
            )));
        }
        let subject = res.json::<Vec<RelatedSubject>>().await?;
        Ok(subject)
    }

    pub async fn get_epsiodes(&self, id: i64) -> Result<Page<Episode>> {
        let url = format!(
            "https://api.bgm.tv/v0/episodes?subject_id={}&limit=100&offset=0",
            id
        );
        let res = self.client.get(&url).send().await?;
        if res.status() != 200 {
            return Err(Error::msg(format!(
                "bgm get episodes failed, http status code is {}",
                res.status()
            )));
        }
        Ok(res.json().await?)
    }

    pub async fn search(&self, keyword: &str) -> Result<Page<BangumiSubject>> {
        let url = "https://api.bgm.tv/v0/search/subjects";
        let body = SearchQuery {
            keyword: keyword.to_string(),
            sort: "rank".to_string(),
            filter: FilterConfig {
                r#type: vec![SubjectType::Anime],
                nsfw: true,
            },
        };
        let res = self.client.post(url).json(&body).send().await?;
        if res.status() != 200 {
            return Err(Error::msg(format!(
                "bgm search {} failed, http status code is {}",
                keyword,
                res.status()
            )));
        }
        Ok(res.json().await?)
    }
}

/// 判断两个字符串相似度
/// 使用 NFKD 处理 Unicode
pub fn is_str_match(query: &str, tmdb_title: &str) -> f64 {
    let iter_query = query.nfkd().flat_map(|c| c.to_lowercase());
    let iter_tmdb = tmdb_title.nfkd().flat_map(|c| c.to_lowercase());

    // 基于迭代器的极低开销实现
    let mut prev_row: Vec<usize> = vec![0];
    for _ in iter_tmdb.clone() {
        prev_row.push(prev_row.len());
    }
    let tmdb_len = prev_row.len() - 1;

    let mut curr_row = vec![0; tmdb_len + 1];
    let mut query_len = 0;

    for char_q in iter_query {
        query_len += 1;
        curr_row[0] = query_len;

        for (j, char_t) in iter_tmdb.clone().enumerate() {
            let cost = if char_q == char_t { 0 } else { 1 };
            curr_row[j + 1] = (curr_row[j] + 1)
                .min(prev_row[j + 1] + 1)
                .min(prev_row[j] + cost);
        }
        prev_row.copy_from_slice(&curr_row);
    }

    let max_len = query_len.max(tmdb_len);

    if max_len == 0 {
        return 1.0;
    }

    let distance = prev_row[tmdb_len];

    (max_len - distance) as f64 / max_len as f64
}

fn to_keywords(titles: &[&str]) -> Result<Vec<String>> {
    // Pass 1: 删掉不要的。合并“括号及内容”、“季度词汇”、“结尾纯数字”
    let re_junk = regex::Regex::new(
        r"(?ix)(
            \([^)]*\)|（[^）]*） |  # 连带括号内容一起干掉（TMDB不需要括号里的副标题）
            第[0-9一二三四五六七八九十]+[期季部章クール]+ |
            \b\d+(?:st|nd|rd|th)\s*Season\b |
            \bSeason\s*\d+\b |
            シーズン\s*\d+ |
            [ⅡⅢⅣⅤⅥⅦⅧⅨⅩ]+ |
            \s*\d+\s*$           # 专门切掉结尾残留的阿拉伯数字
        )",
    )?;

    // Pass 2: 核心白名单。匹配所有【非文字、非数字】的字符，将其变为空格
    // \p{L} = Letter（涵盖汉字、平假名、片假名含长音符、英文字母）
    // \p{N} = Number（保留中间的正常数字，比如“100人の彼女”）
    // [^...] 配合 + 号，会自动将连续的各种奇怪符号、全半角空格全部合并为一个单空格
    let re_whitelist = regex::Regex::new(r"[^\p{L}\p{N}]+")?;

    Ok(titles
        .iter()
        .map(|&title| {
            // 第一步：切掉季度和结尾数字
            let step1 = re_junk.replace_all(title, "");

            // 第二步：除了中英日文和数字，其余所有符号转成标准空格，然后去掉首尾空格
            re_whitelist.replace_all(&step1, " ").trim().to_string()
        })
        .collect())
}
