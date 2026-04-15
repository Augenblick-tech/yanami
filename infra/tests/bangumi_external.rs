use infra::bangumi::BangumiClient;

#[tokio::test]
#[ignore = "requires live Bangumi network access"]
async fn live_fetches_calendar_from_bangumi() {
    let client = BangumiClient::new().expect("create bangumi client");

    let calendar = client.get_calendar().await.expect("fetch bangumi calendar");

    assert!(
        !calendar.is_empty(),
        "expected bangumi calendar to return at least one weekday bucket"
    );
    assert!(
        calendar
            .iter()
            .any(|day| day.weekday.id > 0 && !day.items.is_empty()),
        "expected bangumi calendar to contain at least one non-empty weekday bucket"
    );
}

#[tokio::test]
#[ignore = "requires live Bangumi network access"]
async fn live_fetches_calendar_anime_from_bangumi() {
    let client = BangumiClient::new().expect("create bangumi client");

    let anime = client
        .get_calendar_anime()
        .await
        .expect("fetch bangumi calendar anime");

    assert!(
        !anime.is_empty(),
        "expected bangumi calendar anime to contain at least one item"
    );

    for item in anime.iter().take(10) {
        assert!(item.id > 0, "expected positive bangumi subject id");
        assert!(
            !item.name.trim().is_empty(),
            "expected bangumi subject name"
        );
        assert!(item.weekday > 0, "expected positive bangumi weekday id");
        assert!(
            !item.air_date.trim().is_empty(),
            "expected bangumi air date"
        );
    }
}

#[tokio::test]
#[ignore = "requires live Bangumi network access"]
async fn live_searches_anime_subjects_from_bangumi() {
    let client = BangumiClient::new().expect("create bangumi client");

    let page = client
        .search_anime("葬送", 10, 0)
        .await
        .expect("search bangumi anime");

    assert!(page.total > 0, "expected bangumi anime search total > 0");
    assert!(!page.data.is_empty(), "expected bangumi anime search data");
    assert!(
        page.data.iter().all(|subject| subject.subject_type == 2),
        "expected bangumi anime search to only return anime subjects"
    );
    assert!(
        page.data
            .iter()
            .any(|subject| !subject.name.trim().is_empty() || !subject.name_cn.trim().is_empty()),
        "expected bangumi anime search result to contain names"
    );
}
