use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Error, Result, anyhow};
use chrono::{Datelike, Days, NaiveDate};
use regex::Regex;
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

/// 新番列表提前下一季开始的天数
const SYNC_LEAD_DAYS: u64 = 10;

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
        let keywords = to_keywords(&names);
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
                // 关键词是清洗过的番剧名（去掉了季度词、篇章词、括号副标题），
                // 拿它和结果名字比对最能反映是不是同一部剧。少了这一步，
                // 清洗后搜出来的「…新编集版」这类更长的变体会因为分母更大而得分更高。
                score = score
                    .max(is_str_match(&key, &item.inner.name))
                    .max(is_str_match(&key, &item.inner.original_name));
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

    /// 同步新番列表时使用的季度取值：比自然季度提前 [`SYNC_LEAD_DAYS`] 天进入下一季。
    /// bangumi-data 一般在季度开始前十天左右才发布下一季的月度文件，
    /// 例：9 月 21 日之前仍取 7 月新番列表，9 月 21 日起改取 10 月新番列表。
    pub(super) fn sync_season_of_date(date: &NaiveDate) -> Result<(i32, u32)> {
        let target = date
            .checked_add_days(Days::new(SYNC_LEAD_DAYS))
            .ok_or_else(|| anyhow!("add {SYNC_LEAD_DAYS} days to {date} failed"))?;
        Self::season_of_date(&target)
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

/// 剧名噪音：括号副标题、季度词、罗马数字、结尾残留的阿拉伯数字。
/// 正则编译很贵，所以静态化（与 `crates/feed/src/infra/feed.rs` 的 `COLLECTION_RE` 同一写法），
/// 只编译一次、之后每次调用都是原子读 + 复用。
fn junk_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            \([^)]*\)|（[^）]*） |      # 连带括号内容一起干掉（TMDB不需要括号里的副标题）
            第[0-9一二三四五六七八九十]+[期季部章クール]+ |
            \b\d+(?:st|nd|rd|th)\s*Season\b |
            \bSeason\s*\d+\b |
            シーズン\s*\d+ |
            [ⅡⅢⅣⅤⅥⅦⅧⅨⅩ]+ |
            \s*\d+\s*$                 # 专门切掉结尾残留的阿拉伯数字
        ",
        )
        .expect("junk regex must be valid")
    })
}

/// 篇章词：袭击篇 / 襲擊編 / 第2部 / Part 2。TMDB 的剧集名不带篇章后缀，
/// 这类词留在关键词里会让搜索结果为空，所以要能单独摘掉。
fn arc_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            \p{Han}{1,8}[篇編] |
            第[0-9一二三四五六七八九十]+[篇編部] |
            \b(?:Part|Cour|Cou)\s*\d+\b |
            \b\d+(?:st|nd|rd|th)\s*(?:Part|Cour)\b
        ",
        )
        .expect("arc regex must be valid")
    })
}

/// 除中英日文字与数字外，其余字符（含各种全半角空格）统一压成一个空格并去掉首尾空格。
/// 一次字符扫描，比原来的白名单正则快，也不产生中间字符串。
fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
    }
    let len = out.trim_end().len();
    out.truncate(len);
    out
}

/// 主关键词：切掉季度词、结尾数字与标点。保留篇章词——
/// 真把篇章词写进剧名的作品要靠它命中。
fn clean(title: &str) -> String {
    normalize(&junk_re().replace_all(title, ""))
}

/// 去掉篇章词后的关键词：TMDB 上真正搜得到的那一条。
fn clean_without_arc(title: &str) -> String {
    let without_arc = arc_re().replace_all(title, " ");
    normalize(&junk_re().replace_all(&without_arc, ""))
}

fn push_unique(keywords: &mut Vec<String>, candidate: String) {
    if !candidate.is_empty() && !keywords.contains(&candidate) {
        keywords.push(candidate);
    }
}

fn to_keywords(titles: &[&str]) -> Vec<String> {
    let mut keywords: Vec<String> = Vec::with_capacity(titles.len() * 2);
    for &title in titles {
        push_unique(&mut keywords, clean(title));
        // 篇章词在 TMDB 上一条都搜不到，再补一条去掉它的候选：
        // 「Re：从零开始的异世界生活 第三季 袭击篇」→「Re 从零开始的异世界生活」才搜得到这部剧。
        if arc_re().is_match(title) {
            push_unique(&mut keywords, clean_without_arc(title));
        }
    }
    keywords
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn season_of_date_maps_months_to_quarter_starts() {
        // 每季度的首尾月份都必须落到同一个季号
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 1, 1)).unwrap(),
            (2026, 1)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 3, 31)).unwrap(),
            (2026, 1)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 4, 1)).unwrap(),
            (2026, 4)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 6, 30)).unwrap(),
            (2026, 4)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 7, 1)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 9, 30)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 10, 1)).unwrap(),
            (2026, 10)
        );
        assert_eq!(
            BgmClient::season_of_date(&date(2026, 12, 31)).unwrap(),
            (2026, 10)
        );

        // 年份原样带出
        assert_eq!(
            BgmClient::season_of_date(&date(2008, 4, 6)).unwrap(),
            (2008, 4)
        );
    }

    #[test]
    fn sync_season_of_date_enters_next_quarter_ten_days_early() {
        // 距下一季开始还有 11 天时，仍然取上一季的新番列表
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 9, 20)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 6, 20)).unwrap(),
            (2026, 4)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 3, 21)).unwrap(),
            (2026, 1)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 12, 21)).unwrap(),
            (2026, 10)
        );

        // 距下一季开始整好 10 天起，改为取下一季的新番列表
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 9, 21)).unwrap(),
            (2026, 10)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 6, 21)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 3, 22)).unwrap(),
            (2026, 4)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 12, 22)).unwrap(),
            (2027, 1)
        );

        // 季度中间始终取本季的新番列表
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 7, 1)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 8, 31)).unwrap(),
            (2026, 7)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 10, 2)).unwrap(),
            (2026, 10)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 11, 30)).unwrap(),
            (2026, 10)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2026, 1, 31)).unwrap(),
            (2026, 1)
        );
        assert_eq!(
            BgmClient::sync_season_of_date(&date(2027, 2, 28)).unwrap(),
            (2027, 1)
        );
    }

    #[test]
    fn is_str_match_is_one_for_identical_titles() {
        let title = "オールワークスメイドです";
        assert_close(is_str_match(title, title), 1.0);

        // 两边都为空时按完全匹配处理
        assert_close(is_str_match("", ""), 1.0);
    }

    #[test]
    fn is_str_match_normalizes_case_and_full_width() {
        // NFKD 把全角拉丁字母还原成半角，再统一小写
        assert_close(
            is_str_match("ＧＡＭＥ ＯＦ ＴＨＲＯＮＥＳ", "game of thrones"),
            1.0,
        );
        assert_close(
            is_str_match("Ｈｅｒｏｉｎｅ？Ｓａｉｎｔ？", "heroine?saint?"),
            1.0,
        );
        // 大小写差异不影响结果
        assert_close(is_str_match("Game of Thrones", "GAME OF THRONES"), 1.0);
    }

    #[test]
    fn is_str_match_scores_partial_edits_by_distance() {
        // 一个字符之差：相似度 = (3 - 1) / 3
        assert_close(is_str_match("abc", "abd"), 2.0 / 3.0);

        // 前缀缺失时介于 0 与 1 之间
        let score = is_str_match("Spy x Family", "Spy Family");
        assert!(score > 0.8 && score < 1.0, "unexpected score {score}");
    }

    #[test]
    fn is_str_match_is_low_for_unrelated_titles() {
        let score = is_str_match("Game of Thrones", "ワンピース");
        assert!(score < 0.4, "unexpected score {score}");

        let score = is_str_match("女主角？圣女？", "Game of Thrones");
        assert!(score < 0.4, "unexpected score {score}");
    }

    #[test]
    fn to_keywords_adds_arc_stripped_keyword_for_season_arc_title() {
        // 真实案例：bgm subject 425998（Re:Zero 第三季 襲擊編）。
        // 带「袭击篇」的关键词在 TMDB 上 total_results = 0，
        // 只有去掉篇章词的「Re 从零开始的异世界生活」才搜得到这部剧。
        let keywords = to_keywords(&["Re：从零开始的异世界生活 第三季 袭击篇"]);

        assert_eq!(
            keywords,
            vec![
                "Re 从零开始的异世界生活 袭击篇".to_string(),
                "Re 从零开始的异世界生活".to_string(),
            ]
        );
    }

    #[test]
    fn to_keywords_strips_japanese_arc_marker() {
        // 原名里的「襲擊編」同样带不出 TMDB 结果，额外给一条去掉它的关键词
        let keywords = to_keywords(&["Re:ゼロから始める異世界生活 3rd season 襲擊編"]);

        assert_eq!(
            keywords,
            vec![
                "Re ゼロから始める異世界生活 襲擊編".to_string(),
                "Re ゼロから始める異世界生活".to_string(),
            ]
        );
    }

    #[test]
    fn to_keywords_keeps_titles_without_arc_word_unchanged() {
        // 没有篇章词时只产出一条关键词，不额外发请求
        let keywords = to_keywords(&["进击的巨人"]);

        assert_eq!(keywords, vec!["进击的巨人".to_string()]);
    }

    #[test]
    fn to_keywords_keeps_season_word_behavior() {
        // 回归：季度词仍然被 Pass 1 切掉，且不会因为英文标题多出关键词
        let keywords = to_keywords(&["Re:ZERO -Starting Life in Another World- Season 3"]);

        assert_eq!(
            keywords,
            vec!["Re ZERO Starting Life in Another World".to_string()]
        );
    }

    #[test]
    fn to_keywords_strips_part_and_cour_marker() {
        // Part / Cour 这类英文篇章词同样去掉，原关键词保留
        let keywords = to_keywords(&["進撃の巨人 The Final Season Part 2"]);

        assert_eq!(
            keywords,
            vec![
                "進撃の巨人 The Final Season Part".to_string(),
                "進撃の巨人 The Final Season".to_string(),
            ]
        );
    }

    #[test]
    fn to_keywords_drops_digit_arc_duplicate() {
        // 「第2部」这类阿拉伯数字篇章词：Pass 1 已经切掉，补充的关键词与主关键词相同，
        // 去重后只剩一条
        let keywords = to_keywords(&["某番剧 第2部"]);

        assert_eq!(keywords, vec!["某番剧".to_string()]);
    }

    #[test]
    fn to_keywords_deduplicates_repeated_titles() {
        let keywords = to_keywords(&["进击的巨人", "进击的巨人"]);

        assert_eq!(keywords, vec!["进击的巨人".to_string()]);
    }

    #[test]
    fn normalize_collapses_punctuation_runs_into_single_space() {
        // 原来是白名单正则（Pass 2），改成手写扫描后行为不变：
        // 连续标点/全半角空格压成一个空格，首尾不留空格，中间的数字保留
        assert_eq!(
            normalize("　Re：从零开始的异世界生活　第三季,,"),
            "Re 从零开始的异世界生活 第三季"
        );
        assert_eq!(normalize("  a,,,b 　 c  "), "a b c");
        assert_eq!(normalize("100人の彼女"), "100人の彼女");
    }
}
