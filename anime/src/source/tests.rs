use std::sync::{Arc, Mutex};

use crate::gateway::{
    TmdbAlternativeTitleItem, TmdbAlternativeTitles, TmdbSearchResult, TmdbSearchResultItem,
    TmdbSeason, TmdbSeriesDetails,
};
use crate::source::{
    AnimeMetadataSeed, LoadTmdbAlternativeTitles, LoadTmdbSeriesDetails, SearchTmdbTv,
};
use domain::shared::error::DomainError;

use super::build_anime_metadata;

struct StubTmdbGateway {
    search_results: Vec<TmdbSearchResultItem>,
    series: TmdbSeriesDetails,
    titles: TmdbAlternativeTitles,
    searched_keywords: Mutex<Vec<String>>,
}

fn build_tmdb_search(tmdb: Arc<StubTmdbGateway>) -> Arc<SearchTmdbTv> {
    Arc::new(move |query, _language| {
        let tmdb = tmdb.clone();
        Box::pin(async move {
            tmdb.searched_keywords
                .lock()
                .expect("searched keywords")
                .push(query.to_string());
            Ok::<TmdbSearchResult, DomainError>(TmdbSearchResult {
                results: tmdb.search_results.clone(),
            })
        })
    })
}

fn build_tmdb_series_details(tmdb: Arc<StubTmdbGateway>) -> Arc<LoadTmdbSeriesDetails> {
    Arc::new(move |_series_id, _language| {
        let tmdb = tmdb.clone();
        Box::pin(async move { Ok::<TmdbSeriesDetails, DomainError>(tmdb.series.clone()) })
    })
}

fn build_tmdb_titles(tmdb: Arc<StubTmdbGateway>) -> Arc<LoadTmdbAlternativeTitles> {
    Arc::new(move |_series_id| {
        let tmdb = tmdb.clone();
        Box::pin(async move { Ok::<TmdbAlternativeTitles, DomainError>(tmdb.titles.clone()) })
    })
}

#[tokio::test]
async fn fetches_metadata_by_composing_bangumi_and_tmdb() {
    let tmdb = Arc::new(StubTmdbGateway {
        search_results: vec![TmdbSearchResultItem {
            id: 99,
            name: Some("葬送的芙莉蓮".to_string()),
            original_language: Some("ja".to_string()),
            first_air_date: Some("2026-01-05".to_string()),
        }],
        series: TmdbSeriesDetails {
            first_air_date: Some("2026-01-05".to_string()),
            name: Some("葬送的芙莉莲".to_string()),
            seasons: vec![TmdbSeason {
                episode_count: 24,
                season_number: 2,
            }],
        },
        titles: TmdbAlternativeTitles {
            results: vec![TmdbAlternativeTitleItem {
                title: "Frieren".to_string(),
            }],
        },
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![AnimeMetadataSeed {
        id: 7,
        name: "葬送のフリーレン 第2期".to_string(),
        weekday: 5,
        eps: None,
        air_date: "2026-01-10".to_string(),
    }];

    let metadata = build_anime_metadata(
        &seeds,
        build_tmdb_search(tmdb.clone()).as_ref(),
        build_tmdb_series_details(tmdb.clone()).as_ref(),
        build_tmdb_titles(tmdb).as_ref(),
    )
    .await
    .expect("metadata");

    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].id.0, 7);
    assert_eq!(metadata[0].titles.search_name, "葬送のフリーレン");
    assert_eq!(metadata[0].planned_episode_count.0, 24);
    assert_eq!(metadata[0].season.0, 2);
}

#[tokio::test]
async fn skips_non_japanese_tmdb_results() {
    let tmdb = Arc::new(StubTmdbGateway {
        search_results: vec![TmdbSearchResultItem {
            id: 99,
            name: Some("测试".to_string()),
            original_language: Some("zh".to_string()),
            first_air_date: Some("2026-01-05".to_string()),
        }],
        series: TmdbSeriesDetails {
            first_air_date: Some("2026-01-05".to_string()),
            name: Some("测试".to_string()),
            seasons: vec![TmdbSeason {
                episode_count: 12,
                season_number: 1,
            }],
        },
        titles: TmdbAlternativeTitles { results: vec![] },
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![AnimeMetadataSeed {
        id: 7,
        name: "测试".to_string(),
        weekday: 5,
        eps: Some(12),
        air_date: "2026-01-10".to_string(),
    }];

    let metadata = build_anime_metadata(
        &seeds,
        build_tmdb_search(tmdb.clone()).as_ref(),
        build_tmdb_series_details(tmdb.clone()).as_ref(),
        build_tmdb_titles(tmdb).as_ref(),
    )
    .await
    .expect("metadata");

    assert!(metadata.is_empty());
}

#[tokio::test]
async fn strips_season_suffix_and_uses_trimmed_keyword() {
    let tmdb = Arc::new(StubTmdbGateway {
        search_results: vec![TmdbSearchResultItem {
            id: 42,
            name: Some("百鬼夜行抄".to_string()),
            original_language: Some("ja".to_string()),
            first_air_date: Some("2026-04-07".to_string()),
        }],
        series: TmdbSeriesDetails {
            first_air_date: Some("2026-04-07".to_string()),
            name: Some("百鬼夜行抄".to_string()),
            seasons: vec![TmdbSeason {
                episode_count: 12,
                season_number: 1,
            }],
        },
        titles: TmdbAlternativeTitles { results: vec![] },
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![AnimeMetadataSeed {
        id: 7,
        name: "百鬼夜行抄 第2期".to_string(),
        weekday: 2,
        eps: None,
        air_date: "2026-04-07".to_string(),
    }];

    let metadata = build_anime_metadata(
        &seeds,
        build_tmdb_search(tmdb.clone()).as_ref(),
        build_tmdb_series_details(tmdb.clone()).as_ref(),
        build_tmdb_titles(tmdb).as_ref(),
    )
    .await
    .expect("metadata");

    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].titles.search_name, "百鬼夜行抄");
}

#[tokio::test]
async fn retries_tmdb_search_after_trimming_trailing_number() {
    struct RetryTmdbGateway {
        searched_keywords: Mutex<Vec<String>>,
    }

    let tmdb = Arc::new(RetryTmdbGateway {
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![AnimeMetadataSeed {
        id: 8,
        name: "启示录酒店 2".to_string(),
        weekday: 3,
        eps: None,
        air_date: "2026-04-08".to_string(),
    }];

    let metadata = build_anime_metadata(
        &seeds,
        &{
            let tmdb = tmdb.clone();
            move |query, _language| {
                let tmdb = tmdb.clone();
                Box::pin(async move {
                    tmdb.searched_keywords
                        .lock()
                        .expect("searched keywords")
                        .push(query.to_string());
                    if query.ends_with('2') {
                        return Ok::<TmdbSearchResult, DomainError>(TmdbSearchResult {
                            results: vec![],
                        });
                    }
                    Ok::<TmdbSearchResult, DomainError>(TmdbSearchResult {
                        results: vec![TmdbSearchResultItem {
                            id: 77,
                            name: Some("启示录酒店".to_string()),
                            original_language: Some("ja".to_string()),
                            first_air_date: Some("2026-04-08".to_string()),
                        }],
                    })
                })
            }
        },
        &(move |_series_id, _language| {
            Box::pin(async move {
                Ok::<TmdbSeriesDetails, DomainError>(TmdbSeriesDetails {
                    first_air_date: Some("2026-04-08".to_string()),
                    name: Some("启示录酒店".to_string()),
                    seasons: vec![TmdbSeason {
                        episode_count: 12,
                        season_number: 1,
                    }],
                })
            })
        }),
        &(move |_series_id| {
            Box::pin(async move {
                Ok::<TmdbAlternativeTitles, DomainError>(TmdbAlternativeTitles { results: vec![] })
            })
        }),
    )
    .await
    .expect("metadata");

    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].titles.search_name, "启示录酒店");
}

#[tokio::test]
async fn skips_entries_when_series_name_or_tw_name_is_missing() {
    let tmdb = Arc::new(StubTmdbGateway {
        search_results: vec![TmdbSearchResultItem {
            id: 9,
            name: None,
            original_language: Some("ja".to_string()),
            first_air_date: Some("2026-01-01".to_string()),
        }],
        series: TmdbSeriesDetails {
            first_air_date: Some("2026-01-01".to_string()),
            name: None,
            seasons: vec![TmdbSeason {
                episode_count: 12,
                season_number: 1,
            }],
        },
        titles: TmdbAlternativeTitles { results: vec![] },
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![
        AnimeMetadataSeed {
            id: 7,
            name: "A".to_string(),
            weekday: 1,
            eps: Some(12),
            air_date: "2026-01-01".to_string(),
        },
        AnimeMetadataSeed {
            id: 8,
            name: "B".to_string(),
            weekday: 1,
            eps: Some(12),
            air_date: "2026-01-01".to_string(),
        },
    ];

    let metadata = build_anime_metadata(
        &seeds,
        build_tmdb_search(tmdb.clone()).as_ref(),
        build_tmdb_series_details(tmdb.clone()).as_ref(),
        build_tmdb_titles(tmdb).as_ref(),
    )
    .await
    .expect("metadata");

    assert!(metadata.is_empty());
}

#[tokio::test]
async fn skips_entries_when_tmdb_series_has_invalid_season_or_episode_count() {
    let tmdb = Arc::new(StubTmdbGateway {
        search_results: vec![TmdbSearchResultItem {
            id: 9,
            name: Some("A".to_string()),
            original_language: Some("ja".to_string()),
            first_air_date: Some("2026-01-01".to_string()),
        }],
        series: TmdbSeriesDetails {
            first_air_date: Some("2026-01-01".to_string()),
            name: Some("A".to_string()),
            seasons: vec![TmdbSeason {
                episode_count: 0,
                season_number: 0,
            }],
        },
        titles: TmdbAlternativeTitles { results: vec![] },
        searched_keywords: Mutex::new(vec![]),
    });

    let seeds = vec![
        AnimeMetadataSeed {
            id: 7,
            name: "A".to_string(),
            weekday: 1,
            eps: None,
            air_date: "2026-01-01".to_string(),
        },
        AnimeMetadataSeed {
            id: 8,
            name: "B".to_string(),
            weekday: 1,
            eps: None,
            air_date: "2026-01-01".to_string(),
        },
    ];

    let metadata = build_anime_metadata(
        &seeds,
        build_tmdb_search(tmdb.clone()).as_ref(),
        build_tmdb_series_details(tmdb.clone()).as_ref(),
        build_tmdb_titles(tmdb).as_ref(),
    )
    .await
    .expect("metadata");

    assert!(metadata.is_empty());
}

#[test]
fn select_tmdb_result_prefers_same_month_and_falls_back_to_first() {
    let seed = AnimeMetadataSeed {
        id: 1,
        name: "test".to_string(),
        weekday: 1,
        eps: Some(12),
        air_date: "2026-04-07".to_string(),
    };
    let results = vec![
        TmdbSearchResultItem {
            id: 1,
            name: Some("old".to_string()),
            original_language: Some("ja".to_string()),
            first_air_date: Some("2025-10-01".to_string()),
        },
        TmdbSearchResultItem {
            id: 2,
            name: Some("match".to_string()),
            original_language: Some("ja".to_string()),
            first_air_date: Some("2026-04-01".to_string()),
        },
    ];

    let matched = super::select_tmdb_result(&seed, &results).expect("match");
    assert_eq!(matched.id, 2);

    let fallback = super::select_tmdb_result(
        &AnimeMetadataSeed {
            air_date: "invalid".to_string(),
            ..seed
        },
        &results,
    )
    .expect("fallback");
    assert_eq!(fallback.id, 1);
}

#[test]
fn select_air_date_prefers_earlier_calendar_date_and_series_date_otherwise() {
    let seed = AnimeMetadataSeed {
        id: 1,
        name: "test".to_string(),
        weekday: 1,
        eps: Some(12),
        air_date: "2026-04-01".to_string(),
    };
    let series = TmdbSeriesDetails {
        first_air_date: Some("2026-04-07".to_string()),
        name: Some("test".to_string()),
        seasons: vec![],
    };

    assert_eq!(super::select_air_date(&seed, &series), "2026-04-01");

    let later_seed = AnimeMetadataSeed {
        air_date: "2026-04-10".to_string(),
        ..seed
    };
    assert_eq!(super::select_air_date(&later_seed, &series), "2026-04-07");
}
