#[cfg(test)]
mod tests {
    use anime::gateway::{
        BangumiAnimeSearchResultPage, BangumiCalendarAnime, BangumiSubject, BangumiSubjectSmall,
        YucAnimeEntry, YucSeasonCalendarPage,
    };
    use domain::shared::error::DomainError;

    use crate::seed_source::{BangumiSeedSource, YucBangumiSeedSource};

    #[tokio::test]
    async fn resolves_yuc_entries_into_standard_seeds() {
        let source = YucBangumiSeedSource::from_loaders(
            || async {
                Ok::<YucSeasonCalendarPage, DomainError>(YucSeasonCalendarPage {
                    page_title: "2026年4月新番表".to_string(),
                    page_url: "https://yuc.wiki/202604/".to_string(),
                    season_code: "202604".to_string(),
                    updated_at: None,
                    entries: vec![YucAnimeEntry {
                        title_zh: "葬送的芙莉莲 第2期".to_string(),
                        title_original: Some("葬送のフリーレン 第2期".to_string()),
                        image_url: None,
                        source_type: None,
                        genre_text: None,
                        original_work: None,
                        original_work_extra: None,
                        schedule_weekday: None,
                        schedule_time: None,
                        schedule_start_date: Some("4/6~".to_string()),
                        schedule_note: None,
                        broadcast_text: Some("4/6周日晚间".to_string()),
                        streaming_links: vec![],
                        resource_links: vec![],
                        staff: vec![],
                        cast: vec![],
                    }],
                })
            },
            |_year, _month| async {
                Ok::<YucSeasonCalendarPage, DomainError>(YucSeasonCalendarPage {
                    page_title: "2026年4月新番表".to_string(),
                    page_url: "https://yuc.wiki/202604/".to_string(),
                    season_code: "202604".to_string(),
                    updated_at: None,
                    entries: vec![],
                })
            },
            |_keyword, _limit, _offset| async {
                Ok::<BangumiAnimeSearchResultPage, DomainError>(BangumiAnimeSearchResultPage {
                    total: 1,
                    limit: 10,
                    offset: 0,
                    data: vec![BangumiSubjectSmall {
                        id: 77,
                        subject_type: 2,
                        name: "葬送のフリーレン 第2期".to_string(),
                        name_cn: "葬送的芙莉莲 第2期".to_string(),
                        summary: String::new(),
                        air_date: Some("2026-04-06".to_string()),
                        air_weekday: Some(7),
                        eps: Some(24),
                    }],
                })
            },
            |_id| async {
                Ok::<Option<BangumiSubject>, DomainError>(Some(BangumiSubject {
                    name: None,
                    name_cn: None,
                    eps: Some(24),
                    air_date: Some("2026-04-06".to_string()),
                }))
            },
        );

        let seeds = source.fetch_anime_metadata_seeds().await.expect("seeds");

        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].id, 77);
        assert_eq!(seeds[0].air_date, "2026-04-06");
        assert_eq!(seeds[0].weekday, 7);
    }

    #[tokio::test]
    async fn resolves_yuc_weekday_from_bangumi_air_date_when_search_result_weekday_is_missing() {
        let source = YucBangumiSeedSource::from_loaders(
            || async {
                Ok::<YucSeasonCalendarPage, DomainError>(YucSeasonCalendarPage {
                    page_title: "2026年4月新番表".to_string(),
                    page_url: "https://yuc.wiki/202604/".to_string(),
                    season_code: "202604".to_string(),
                    updated_at: None,
                    entries: vec![YucAnimeEntry {
                        title_zh: "魔法姐妹露露与莉莉".to_string(),
                        title_original: Some("魔法の姉妹ルルットリリィ".to_string()),
                        image_url: None,
                        source_type: None,
                        genre_text: None,
                        original_work: None,
                        original_work_extra: None,
                        schedule_weekday: None,
                        schedule_time: None,
                        schedule_start_date: Some("4/6~".to_string()),
                        schedule_note: None,
                        broadcast_text: Some("4/6周一晚间".to_string()),
                        streaming_links: vec![],
                        resource_links: vec![],
                        staff: vec![],
                        cast: vec![],
                    }],
                })
            },
            |_year, _month| async {
                Ok::<YucSeasonCalendarPage, DomainError>(YucSeasonCalendarPage {
                    page_title: "2026年4月新番表".to_string(),
                    page_url: "https://yuc.wiki/202604/".to_string(),
                    season_code: "202604".to_string(),
                    updated_at: None,
                    entries: vec![],
                })
            },
            |_keyword, _limit, _offset| async {
                Ok::<BangumiAnimeSearchResultPage, DomainError>(BangumiAnimeSearchResultPage {
                    total: 1,
                    limit: 10,
                    offset: 0,
                    data: vec![BangumiSubjectSmall {
                        id: 501796,
                        subject_type: 2,
                        name: "魔法の姉妹ルルットリリィ".to_string(),
                        name_cn: "魔法姐妹露露与莉莉".to_string(),
                        summary: String::new(),
                        air_date: Some("2026-04-06".to_string()),
                        air_weekday: None,
                        eps: Some(12),
                    }],
                })
            },
            |_id| async {
                Ok::<Option<BangumiSubject>, DomainError>(Some(BangumiSubject {
                    name: None,
                    name_cn: None,
                    eps: Some(12),
                    air_date: Some("2026-04-06".to_string()),
                }))
            },
        );

        let seeds = source.fetch_anime_metadata_seeds().await.expect("seeds");

        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].id, 501796);
        assert_eq!(seeds[0].weekday, 1);
    }

    #[tokio::test]
    async fn filters_bangumi_calendar_anime_by_requested_season() {
        let source = BangumiSeedSource::from_loader(|| async {
            Ok::<Vec<BangumiCalendarAnime>, DomainError>(vec![
                BangumiCalendarAnime {
                    id: 627137,
                    name: "百鬼夜行抄".to_string(),
                    weekday: 2,
                    eps: Some(12),
                    air_date: "2026-04-07".to_string(),
                },
                BangumiCalendarAnime {
                    id: 975,
                    name: "ONE PIECE".to_string(),
                    weekday: 3,
                    eps: None,
                    air_date: "1999-10-20".to_string(),
                },
            ])
        });

        let seeds = source
            .fetch_season_anime_metadata_seeds(2026, 4)
            .await
            .expect("season seeds");

        assert!(seeds.iter().any(|seed| seed.id == 627137));
        assert!(seeds.iter().all(|seed| seed.air_date.starts_with("2026-04")
            || seed.air_date.starts_with("2026-05")
            || seed.air_date.starts_with("2026-06")));
    }
}
