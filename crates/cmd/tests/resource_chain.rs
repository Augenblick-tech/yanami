//! 功能测试：资源链路（站点 feed 抓取 → 资源入库 → 订阅匹配 → 缺集检测）。
//!
//! 番剧元数据与资源都取自真实抓取结果（`seasonal()` / `nyaa_feed()`）：nyaa 的真实 RSS
//! 有 72 条，其中 `[Doomdos] - 전생했더니 검이었습니다 Ⅱ - 01 ...` 与抓到的当季番剧同一部。

mod common;

use std::sync::Arc;

use cmd::task::check_missing_episodes_task::check_missing_episodes_task;
use cmd::task::get_resource_task::get_resource_and_match_task;
use common::{
    MockAccessPolicy, MockFeedFetcher, NYAA_URL, TestApp, nyaa_feed, nyaa_item,
    nyaa_item_with_episode, sword_anime,
};
use subscription::entity::model::SubAnimeSearchStatus;
use user::entity::model::UserRole;

/// 站点 feed + 真实抓取内容；把参与匹配的那条发布时间对齐到当前时间，
/// 因为生产任务只匹配「最近 3 小时」内发布的资源（只改时间戳，条目内容不动）。
fn captured_site_feed() -> (MockFeedFetcher, [u8; 20]) {
    let mut data = nyaa_feed();
    let target = data
        .items
        .iter_mut()
        .find(|item| item.title.contains("전생했더니"))
        .expect("captured data should contain a real item of the anime");
    target.published_at = chrono::Utc::now().timestamp();
    let info_hash = target.info_hash;
    (MockFeedFetcher::new().with_feed(NYAA_URL, data), info_hash)
}

#[tokio::test]
async fn get_resource_task_fetches_site_feed_and_matches_subscription() {
    let app = TestApp::new().await;
    let user = app.seed_user("res-user", UserRole::User, false).await;
    let space_id = user.data.space_id;

    let anime = app.seed_anime(&sword_anime()).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    let feed = app.seed_feed("nyaa", Some(NYAA_URL), None).await;

    let (fetcher, info_hash) = captured_site_feed();
    let fetcher = Arc::new(fetcher);
    let policy = Arc::new(MockAccessPolicy::allow());

    get_resource_and_match_task(
        app.feeds(fetcher.clone(), policy.clone()),
        app.resources(),
        app.sub_animes(),
    )
    .await
    .expect("resource task should not fail");

    assert_eq!(
        fetcher.requested(),
        vec![NYAA_URL.to_string()],
        "site feed should be fetched exactly once"
    );
    assert_eq!(
        policy.noted(),
        vec![feed.data.id],
        "fetch result should be reported back to the access policy"
    );

    let eps = app.list_eps(sub_anime.id()).await;
    assert_eq!(
        eps.len(),
        1,
        "only one episode should match within the time window"
    );
    assert_eq!(
        eps[0].data.ep.ep_num,
        Some(1.0),
        "episode number should come from 01 in the title"
    );
    assert_eq!(
        eps[0].data.ep.resource_id, info_hash,
        "episode should point to the fetched resource"
    );
    assert!(
        eps[0].extend.title.contains("전생했더니"),
        "episode title should come from the real item, actual {}",
        eps[0].extend.title
    );
}

#[tokio::test]
async fn get_resource_task_skips_denied_feed() {
    let app = TestApp::new().await;
    let user = app.seed_user("deny-user", UserRole::User, false).await;
    let space_id = user.data.space_id;

    let anime = app.seed_anime(&sword_anime()).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;
    app.seed_feed("nyaa", Some(NYAA_URL), None).await;

    let (fetcher, _) = captured_site_feed();
    let fetcher = Arc::new(fetcher);
    let policy = Arc::new(MockAccessPolicy::deny());

    get_resource_and_match_task(
        app.feeds(fetcher.clone(), policy.clone()),
        app.resources(),
        app.sub_animes(),
    )
    .await
    .expect("denied access is not a task error");

    assert!(
        fetcher.requested().is_empty(),
        "denied feed should not be fetched"
    );
    assert!(
        policy.noted().is_empty(),
        "access policy should not be notified without fetching"
    );
    assert!(
        app.list_eps(sub_anime.id()).await.is_empty(),
        "denied feed should not produce episodes"
    );
}

/// 造出「同一番剧两集」的订阅，返回订阅 id。
async fn arrange_two_episodes(app: &TestApp, episode_a: &str, episode_b: &str) -> i64 {
    let user = app.seed_user("gap-user", UserRole::User, false).await;
    let space_id = user.data.space_id;

    let anime = app.seed_anime(&sword_anime()).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;
    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    let base = nyaa_item("전생했더니");
    let saved = app
        .save_feed_items(vec![
            nyaa_item_with_episode(&base, episode_a),
            nyaa_item_with_episode(&base, episode_b),
        ])
        .await;
    assert_eq!(
        saved.len(),
        2,
        "both resources with different episodes should be saved"
    );

    app.match_all_resources(&sub_anime).await;
    let eps = app.list_eps(sub_anime.id()).await;
    assert_eq!(eps.len(), 2, "both episodes should be matched");

    sub_anime.id()
}

#[tokio::test]
async fn check_missing_episodes_enables_search_when_gap_found() {
    let app = TestApp::new().await;
    let sub_anime_id = arrange_two_episodes(&app, "01", "03").await;

    check_missing_episodes_task(app.sub_animes())
        .await
        .expect("missing episode check task should not fail");

    let sub_anime = app
        .sub_animes()
        .find_by_sub_anime_id(sub_anime_id)
        .await
        .expect("query subscription failed")
        .expect("subscription should exist");
    assert_eq!(
        sub_anime.progress(),
        2,
        "episode 01 and 03 should be matched"
    );
    assert!(
        matches!(sub_anime.search_status(), SubAnimeSearchStatus::Pending),
        "gap between 01 and 03 should move to pending search, actual {:?}",
        sub_anime.search_status()
    );
}

#[tokio::test]
async fn check_missing_episodes_keeps_not_search_when_no_gap() {
    let app = TestApp::new().await;
    let sub_anime_id = arrange_two_episodes(&app, "01", "02").await;

    check_missing_episodes_task(app.sub_animes())
        .await
        .expect("missing episode check task should not fail");

    let sub_anime = app
        .sub_animes()
        .find_by_sub_anime_id(sub_anime_id)
        .await
        .expect("query subscription failed")
        .expect("subscription should exist");
    assert_eq!(
        sub_anime.progress(),
        2,
        "episode 01 and 02 should be matched"
    );
    assert!(
        matches!(sub_anime.search_status(), SubAnimeSearchStatus::NotSearch),
        "episodes 01 and 02 are contiguous, should not move to search, actual {:?}",
        sub_anime.search_status()
    );
}
