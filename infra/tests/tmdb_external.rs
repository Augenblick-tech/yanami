use std::env;

use infra::tmdb::TmdbClient;

fn tmdb_bearer_token() -> String {
    env::var("TMDB_BEARER_TOKEN")
        .expect("TMDB_BEARER_TOKEN is required for live TMDB integration tests")
}

#[tokio::test]
#[ignore = "requires live TMDB network access and TMDB_BEARER_TOKEN"]
async fn live_searches_tv_from_tmdb() {
    let client = TmdbClient::new(&tmdb_bearer_token()).expect("create tmdb client");

    let result = client
        .search_tv("葬送的芙莉莲", "zh-TW")
        .await
        .expect("search tmdb tv");

    assert!(
        !result.results.is_empty(),
        "expected tmdb search result to contain at least one entry"
    );
    assert!(
        result.results.iter().any(|item| item.id > 0
            && item
                .name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())),
        "expected tmdb search result to contain a named tv series"
    );
}

#[tokio::test]
#[ignore = "requires live TMDB network access and TMDB_BEARER_TOKEN"]
async fn live_fetches_series_details_and_alternative_titles_from_tmdb() {
    let client = TmdbClient::new(&tmdb_bearer_token()).expect("create tmdb client");
    let search = client
        .search_tv("葬送的芙莉莲", "zh-TW")
        .await
        .expect("search tmdb tv");
    let first = search
        .results
        .into_iter()
        .find(|item| item.id > 0)
        .expect("expected tmdb search result");

    let details = client
        .get_series_details(first.id, "zh-CN")
        .await
        .expect("fetch tmdb series details");
    assert!(
        details
            .name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty()),
        "expected tmdb series details to contain localized series name"
    );
    assert!(
        !details.seasons.is_empty(),
        "expected tmdb series details to contain seasons"
    );

    let titles = client
        .get_alternative_titles(first.id)
        .await
        .expect("fetch tmdb alternative titles");
    assert!(
        titles
            .results
            .iter()
            .any(|item| !item.title.trim().is_empty()),
        "expected tmdb alternative titles to contain at least one title"
    );
}
