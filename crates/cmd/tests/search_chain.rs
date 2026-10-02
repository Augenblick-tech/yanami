//! 功能测试：搜索链路（本地匹配 → 生成搜索委托 → 委托抓取 → 资源落库匹配 → 结束搜索）。
//!
//! 多语言标题、关键词排序、搜索 url 模板都由生产代码处理；feed 抓取结果用真实 nyaa RSS。

mod common;

use std::sync::Arc;

use cmd::task::search_task::{local_match_task, search_task};
use common::{MockAccessPolicy, MockFeedFetcher, TestApp, nyaa_feed, sword_anime};
use subscription::entity::cap::SearchMandateRepository;
use subscription::entity::model::SubAnimeSearchStatus;
use user::entity::model::UserRole;

/// 站内搜索 feed 的 url 模板：生产代码用 formatx 把关键词填进 `{}`。
const SEARCH_URL_TEMPLATE: &str = "https://mikanani.me/RSS/Search?q={}";

/// 造一个待匹配订阅 + 一个搜索 feed，跑本地匹配任务，
/// 返回 (订阅 id, 一条搜索委托 url, 搜索委托条数)。
async fn arrange_search_mandate(app: &TestApp) -> (i64, String, u64) {
    let user = app.seed_user("search-user", UserRole::User, false).await;
    let space_id = user.data.space_id;

    let anime = app.seed_anime(&sword_anime()).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;

    let mut sub_anime = app.subscribe(space_id, anime.data.id).await;
    assert!(sub_anime.enable_search(), "fresh subscription should move from NotSearch to pending match");
    app.sub_animes()
        .save(&sub_anime)
        .await
        .expect("save subscription failed");

    app.seed_feed("mikan-search", None, Some(SEARCH_URL_TEMPLATE))
        .await;

    let fetcher = Arc::new(MockFeedFetcher::new());
    let policy = Arc::new(MockAccessPolicy::allow());
    local_match_task(
        app.sub_animes(),
        app.resources(),
        app.feeds(fetcher.clone(), policy.clone()),
        app.search_mandates(fetcher, policy),
    )
    .await
    .expect("local match task should not fail");

    let mandate = app
        .ctx
        .repo
        .mandate_repo
        .get_one(&[])
        .await
        .expect("query search mandates failed")
        .expect("local match should create a search mandate");
    let mandate_count = app
        .ctx
        .repo
        .mandate_repo
        .count()
        .await
        .expect("count search mandates failed");

    (sub_anime.id(), mandate.data.mandata.url, mandate_count)
}

#[tokio::test]
async fn local_match_task_turns_pending_subscription_into_searching_with_mandate() {
    let app = TestApp::new().await;
    let (sub_anime_id, mandate_url, mandate_count) = arrange_search_mandate(&app).await;

    assert!(
        mandate_url.starts_with("https://mikanani.me/RSS/Search?q="),
        "mandate url should be built from the feed template and the keyword, actual {mandate_url}"
    );
    assert!(
        mandate_count >= 1,
        "each keyword should create one search mandate, actual {mandate_count}"
    );

    let sub_anime = app
        .sub_animes()
        .find_by_sub_anime_id(sub_anime_id)
        .await
        .expect("query subscription failed")
        .expect("subscription should exist");
    assert!(
        matches!(sub_anime.search_status(), SubAnimeSearchStatus::Searching),
        "subscription should stay searching while mandates are pending, actual {:?}",
        sub_anime.search_status()
    );
}

#[tokio::test]
async fn search_task_saves_matched_episode_and_completes_mandate() {
    let app = TestApp::new().await;
    let (sub_anime_id, _, mandate_count) = arrange_search_mandate(&app).await;

    // 搜索委托的 url 由关键词拼出来（一条委托一个关键词），这里用兜底结果应答其中任意一个
    let fetcher = Arc::new(MockFeedFetcher::new().with_any_feed(nyaa_feed()));
    let policy = Arc::new(MockAccessPolicy::allow());

    // 生产调度里每轮只消费一条委托，跑到搜索池清空为止
    let mut ticks = 0u64;
    while app
        .ctx
        .repo
        .mandate_repo
        .count()
        .await
        .expect("count search mandates failed")
        > 0
    {
        search_task(
            app.search_mandates(fetcher.clone(), policy.clone()),
            app.resources(),
            app.sub_animes(),
        )
        .await
        .expect("search task should not fail");
        ticks += 1;
        assert!(ticks <= 64, "search task should consume all {mandate_count} mandates within limited ticks");
    }

    let requested = fetcher.requested();
    assert_eq!(
        requested.len() as u64, mandate_count,
        "every mandate should be fetched exactly once"
    );
    assert!(
        requested
            .iter()
            .all(|url| url.starts_with("https://mikanani.me/RSS/Search?q=")),
        "fetched url should be the templated search url, actual {requested:?}"
    );
    assert!(
        policy.noted().len() as u64 >= mandate_count,
        "every fetch should be reported to the access policy"
    );

    let eps = app.list_eps(sub_anime_id).await;
    assert_eq!(eps.len(), 1, "fetched real item should match exactly one episode");
    assert_eq!(eps[0].data.ep.ep_num, Some(1.0), "episode number should come from 01 in the title");

    assert_eq!(
        app.ctx
            .repo
            .mandate_repo
            .count()
            .await
            .expect("count search mandates failed"),
        0,
        "all mandates should be removed from the pool once completed"
    );
    let sub_anime = app
        .sub_animes()
        .find_by_sub_anime_id(sub_anime_id)
        .await
        .expect("query subscription failed")
        .expect("subscription should exist");
    assert!(
        matches!(sub_anime.search_status(), SubAnimeSearchStatus::NotSearch),
        "subscription should return to not-searching once search is done, actual {:?}",
        sub_anime.search_status()
    );
}

#[tokio::test]
async fn search_task_keeps_mandate_when_policy_denies_access() {
    let app = TestApp::new().await;
    let (_, _, mandate_count) = arrange_search_mandate(&app).await;

    let fetcher = Arc::new(MockFeedFetcher::new().with_any_feed(nyaa_feed()));
    let policy = Arc::new(MockAccessPolicy::deny());
    search_task(
        app.search_mandates(fetcher.clone(), policy),
        app.resources(),
        app.sub_animes(),
    )
    .await
    .expect("denied access is not a task error");

    assert!(
        fetcher.requested().is_empty(),
        "no fetch should be triggered when access is denied"
    );
    assert_eq!(
        app.ctx
            .repo
            .mandate_repo
            .count()
            .await
            .expect("count search mandates failed"),
        mandate_count,
        "denied mandate should stay in the pool for the next tick"
    );
}

#[tokio::test]
async fn search_task_drops_mandate_and_ends_search_when_feed_unreachable() {
    let app = TestApp::new().await;
    let (sub_anime_id, _, mandate_count) = arrange_search_mandate(&app).await;

    let fetcher = Arc::new(MockFeedFetcher::new().with_any_failure());
    let policy = Arc::new(MockAccessPolicy::allow());
    search_task(
        app.search_mandates(fetcher.clone(), policy.clone()),
        app.resources(),
        app.sub_animes(),
    )
    .await
    .expect("unreachable feed is not a task error");

    assert!(
        app.list_eps(sub_anime_id).await.is_empty(),
        "no episode should be produced when nothing is fetched"
    );
    assert_eq!(
        app.ctx
            .repo
            .mandate_repo
            .count()
            .await
            .expect("count search mandates failed"),
        mandate_count - 1,
        "one tick should only drop the mandate whose fetch failed"
    );
    let sub_anime = app
        .sub_animes()
        .find_by_sub_anime_id(sub_anime_id)
        .await
        .expect("query subscription failed")
        .expect("subscription should exist");
    // 同一个番剧每个关键词一条委托，一轮任务只处理一条：
    // 还剩其它委托时订阅必须继续停在搜索中（否则剩下的委托就再也没人消费）
    assert!(
        matches!(sub_anime.search_status(), SubAnimeSearchStatus::Searching),
        "subscription should keep searching while mandates remain, actual {:?}",
        sub_anime.search_status()
    );
}
