use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;
use tracing::error;

use crate::source::AnimeMetadataSeed;
use crate::source::{LoadTmdbAlternativeTitles, LoadTmdbSeriesDetails, SearchTmdbTv};
use domain::{
    anime::{
        AirDate, AnimeId, AnimeMetadata, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount,
        SeasonNumber,
    },
    shared::error::DomainError,
};

use super::tmdb_selector::{select_air_date, select_tmdb_result};

// SAFETY: 以下正则在编译期已知有效
static SEASON_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("第[0-9]+期").expect("season regex")); // SAFETY: 编译期已知有效的正则字面量
static EN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("[Ss]eason.*?$").expect("en regex")); // SAFETY: 编译期已知有效的正则字面量
static EN_ND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+.*?[Ss]eason.*?$").expect("en-nd regex")); // SAFETY: 编译期已知有效的正则字面量
static END_NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+$").expect("end-number regex")); // SAFETY: 编译期已知有效的正则字面量
static COUR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第\d+クール$").expect("cour regex")); // SAFETY: 编译期已知有效的正则字面量

pub async fn build_anime_metadata(
    raw_items: &[AnimeMetadataSeed],
    search_tmdb_tv: &SearchTmdbTv,
    load_tmdb_series_details: &LoadTmdbSeriesDetails,
    load_tmdb_alternative_titles: &LoadTmdbAlternativeTitles,
) -> Result<Vec<AnimeMetadata>, DomainError> {
    let mut snapshots = Vec::new();

    for item in raw_items {
        let mut search_name = SEASON_RE.replace(&item.name, "").trim().to_string();
        search_name = EN_ND_RE.replace(&search_name, "").trim().to_string();
        search_name = COUR_RE.replace(&search_name, "").trim().to_string();
        search_name = EN_RE.replace(&search_name, "").trim().to_string();

        let mut search_result = match search_tmdb_tv(search_name.clone(), "zh-TW".to_string()).await
        {
            Ok(result) => result,
            Err(error) => {
                error!(
                    item_id = item.id,
                    item_name = %item.name,
                    search_name = %search_name,
                    ?error,
                    "tmdb search failed for anime metadata seed, skipping"
                );
                continue;
            }
        };
        if search_result.results.is_empty() {
            search_name = END_NUMBER_RE.replace(&search_name, "").trim().to_string();
            search_result = match search_tmdb_tv(search_name.clone(), "zh-TW".to_string()).await {
                Ok(result) => result,
                Err(error) => {
                    error!(
                        item_id = item.id,
                        item_name = %item.name,
                        search_name = %search_name,
                        ?error,
                        "tmdb retry search failed for anime metadata seed, skipping"
                    );
                    continue;
                }
            };
            if search_result.results.is_empty() {
                tracing::trace!(
                    item_id = item.id,
                    item_name = %item.name,
                    search_name = %search_name,
                    "anime metadata seed skipped: tmdb search returned empty results"
                );
                continue;
            }
        }

        let Some(result) = select_tmdb_result(item, &search_result.results) else {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                search_name = %search_name,
                "anime metadata seed skipped: no matching tmdb result"
            );
            continue;
        };
        if result.original_language.as_deref() != Some("ja") {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                original_language = ?result.original_language,
                "anime metadata seed skipped: not japanese"
            );
            continue;
        }

        let series = match load_tmdb_series_details(result.id, "zh-CN".to_string()).await {
            Ok(series) => series,
            Err(error) => {
                error!(
                    item_id = item.id,
                    item_name = %item.name,
                    tmdb_id = result.id,
                    ?error,
                    "tmdb series details failed, skipping anime"
                );
                continue;
            }
        };
        let Some(season) = series.seasons.last() else {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                "anime metadata seed skipped: series has no season"
            );
            continue;
        };
        if season.season_number <= 0 {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                season_number = season.season_number,
                "anime metadata seed skipped: season number is zero or negative"
            );
            continue;
        }
        let bangumi_eps = item.eps.filter(|eps| *eps > 0);
        if bangumi_eps.is_none() && season.episode_count <= 0 {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                episode_count = season.episode_count,
                "anime metadata seed skipped: no valid episode count"
            );
            continue;
        }

        let alternative_titles = match load_tmdb_alternative_titles(result.id).await {
            Ok(titles) => titles,
            Err(error) => {
                error!(
                    item_id = item.id,
                    item_name = %item.name,
                    tmdb_id = result.id,
                    ?error,
                    "tmdb alternative titles failed, skipping anime"
                );
                continue;
            }
        };
        let mut aliases = alternative_titles
            .results
            .iter()
            .map(|entry| entry.title.clone())
            .collect::<HashSet<String>>();
        aliases.insert(item.name.clone());
        if let Some(name) = &series.name {
            aliases.insert(name.clone());
        }
        if let Some(name) = &result.name {
            aliases.insert(name.clone());
        }

        let Some(localized_zh_cn) = series.name.clone() else {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                "anime metadata seed skipped: series localized zh-CN name is missing"
            );
            continue;
        };
        let Some(localized_zh_tw) = result.name.clone() else {
            tracing::trace!(
                item_id = item.id,
                item_name = %item.name,
                "anime metadata seed skipped: tmdb localized zh-TW name is missing"
            );
            continue;
        };

        snapshots.push(AnimeMetadata {
            id: AnimeId(item.id),
            titles: AnimeTitleSet {
                original_ja: item.name.clone(),
                localized_zh_cn,
                localized_zh_tw,
                search_name,
                aliases: aliases.into_iter().collect(),
            },
            broadcast_weekday: BroadcastWeekday(item.weekday),
            planned_episode_count: PlannedEpisodeCount(match bangumi_eps {
                Some(eps) => eps,
                None => season.episode_count,
            }),
            air_date: AirDate(select_air_date(item, &series)),
            season: SeasonNumber(season.season_number),
        });
    }

    Ok(snapshots)
}
