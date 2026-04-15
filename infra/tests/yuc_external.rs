use infra::yuc::YucClient;

#[tokio::test]
#[ignore = "requires live Yuc network access"]
async fn live_fetches_2026_april_calendar_from_yuc() {
    let client = YucClient::new().expect("create yuc client");

    let page = client
        .fetch_season_calendar(2026, 4)
        .await
        .expect("fetch yuc season calendar");

    assert_eq!(page.season_code, "202604");
    assert!(!page.page_title.trim().is_empty(), "expected page title");
    assert!(
        !page.entries.is_empty(),
        "expected yuc page to contain anime entries"
    );

    let rich_entry = page
        .entries
        .iter()
        .find(|entry| {
            entry.title_original.is_some()
                && entry.source_type.is_some()
                && !entry.staff.is_empty()
                && !entry.cast.is_empty()
        })
        .expect("expected at least one richly parsed anime entry");

    assert!(
        page.entries.iter().any(|entry| entry
            .image_url
            .as_deref()
            .is_some_and(|url| url.starts_with("http"))),
        "expected at least one resolved image url"
    );
    assert!(
        page.entries.iter().any(|entry| {
            entry
                .broadcast_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
        }),
        "expected at least one entry with broadcast text"
    );
    assert!(!rich_entry.title_zh.trim().is_empty(), "expected title");
}
