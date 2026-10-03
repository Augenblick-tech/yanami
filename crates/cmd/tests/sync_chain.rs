//! 功能测试：`sync_calendar_task` 同步链路（来源数据 → 番剧元数据 → 系列 → 用户自动订阅）。

mod common;

use std::sync::Arc;

use anime::entity::cap::AnimeRepository;
use cmd::task::sync_calendar_task::sync_calendar_task;
use common::{
    MockDownloaderManager, MockSeasonalProvider, TestApp, bangumi_id_of, seasonal, tmdb_id_of,
};
use subscription::entity::model::SubAnimeListQuery;
use user::entity::model::UserRole;

/// 同步当季数据后：番剧条目、系列行、自动订阅都应写入。
#[tokio::test]
async fn sync_calendar_writes_anime_series_and_auto_subscription() {
    let app = TestApp::new().await;
    let users = app.users(Arc::new(MockDownloaderManager::new(true)));
    let user = users
        .create(
            "auto-sub-user",
            "test-password-123456",
            UserRole::User,
            true,
        )
        .await
        .expect("create user failed");

    let captured = seasonal();
    assert!(
        !captured.is_empty(),
        "captured seasonal data should not be empty"
    );
    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);

    sync_calendar_task(app.animes(), source, users.clone(), app.sub_animes())
        .await
        .expect("sync_calendar_task failed");

    // 每个抓取条目都应有系列行，且系列原名与抓取结果一致
    for item in &captured {
        let tmdb_id = tmdb_id_of(item).expect("captured seasonal item must carry a TMDB identity");
        let expected = item
            .series_metadata
            .as_ref()
            .expect("captured seasonal item must carry series metadata");
        let series = app
            .ctx
            .repo
            .anime_repo
            .find_series(tmdb_id)
            .await
            .expect("find series failed")
            .expect("sync must write the series row");
        assert_eq!(series.origin_name, expected.origin_name);
        assert_eq!(series.air_date, expected.air_date);
    }

    // 自动订阅：每个自动订阅用户都应订阅到每一条同步下来的番剧
    let sub_list = app
        .sub_animes()
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: Some(user.space_id()),
            search_status: None,
            sub_status: None,
            limit: None,
        })
        .await
        .expect("list sub anime failed");
    assert_eq!(sub_list.len(), captured.len());

    // 订阅行的番剧身份可反查，且能取到它的系列
    for sub in &sub_list {
        let anime = app
            .animes()
            .get(sub.anime_id())
            .await
            .expect("get anime failed")
            .expect("subscribed anime must exist");
        let tmdb_id = anime
            .series_id()
            .expect("anime written by sync must carry a TMDB identity");
        let series = app
            .animes()
            .series(sub.anime_id())
            .await
            .expect("series by anime id failed");
        assert_eq!(series.tmdb_id(), tmdb_id);
    }
}

/// 条目的 Bangumi 身份用于去重：同一份数据同步两次不会产生重复番剧与重复订阅。
#[tokio::test]
async fn sync_calendar_is_idempotent_for_same_source_data() {
    let app = TestApp::new().await;
    let users = app.users(Arc::new(MockDownloaderManager::new(true)));
    let user = users
        .create(
            "auto-sub-user",
            "test-password-123456",
            UserRole::User,
            true,
        )
        .await
        .expect("create user failed");

    let captured = seasonal();
    for _ in 0..2 {
        let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
        sync_calendar_task(app.animes(), source, users.clone(), app.sub_animes())
            .await
            .expect("sync_calendar_task failed");
    }

    let list = app
        .animes()
        .list(anime::entity::model::AnimeListQuery {
            metadata_locked: None,
            keyword: None,
            year: None,
            month: None,
        })
        .await
        .expect("list anime failed");
    assert_eq!(list.len(), captured.len());

    let sub_list = app
        .sub_animes()
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: Some(user.space_id()),
            search_status: None,
            sub_status: None,
            limit: None,
        })
        .await
        .expect("list sub anime failed");
    assert_eq!(sub_list.len(), captured.len());
}

/// 抓取到的元数据发生变化时，同步应把新的系列元数据写回（不是只补缺失）。
#[tokio::test]
async fn sync_calendar_updates_changed_series_metadata() {
    let app = TestApp::new().await;
    let users = app.users(Arc::new(MockDownloaderManager::new(true)));
    users
        .create(
            "auto-sub-user",
            "test-password-123456",
            UserRole::User,
            true,
        )
        .await
        .expect("create user failed");

    let mut captured = seasonal();
    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
    sync_calendar_task(app.animes(), source, users.clone(), app.sub_animes())
        .await
        .expect("first sync failed");

    // 模拟来源侧元数据变动：系列原名与别名都改了
    let (tmdb_id, bangumi_id, changed_origin_name, changed_title) = {
        let changed = &mut captured[0];
        let tmdb_id = tmdb_id_of(changed).expect("captured item must carry a TMDB identity");
        let bangumi_id =
            bangumi_id_of(changed).expect("captured item must carry a Bangumi identity");
        let changed_origin_name = "転生したら剣でした 特別編".to_string();
        changed
            .series_metadata
            .as_mut()
            .expect("captured item must carry series metadata")
            .origin_name = changed_origin_name.clone();
        let changed_title = "轉生就是劍 特別編".to_string();
        // 番剧展示标题取的是原始标题（origin），改动必须落在这一条上
        let origin_title = changed
            .titles
            .iter_mut()
            .find(|t| t.origin)
            .expect("captured item must carry the origin title");
        origin_title.name = changed_title.clone();
        (tmdb_id, bangumi_id, changed_origin_name, changed_title)
    };

    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
    sync_calendar_task(app.animes(), source, users.clone(), app.sub_animes())
        .await
        .expect("second sync failed");

    let series = app
        .ctx
        .repo
        .anime_repo
        .find_series(tmdb_id)
        .await
        .expect("find series failed")
        .expect("series row must exist");
    assert_eq!(series.origin_name, changed_origin_name);

    let anime_id = app
        .ctx
        .repo
        .anime_repo
        .list(&anime::entity::model::AnimeListQuery {
            metadata_locked: None,
            keyword: None,
            year: None,
            month: None,
        })
        .await
        .expect("list anime failed")
        .into_iter()
        .find(|i| bangumi_id_of(&i.data.metadata) == Some(bangumi_id))
        .map(|i| i.data.id)
        .expect("anime written by sync must be listable");
    let anime = app
        .animes()
        .get(anime_id)
        .await
        .expect("get anime failed")
        .expect("anime must exist");
    assert_eq!(anime.title(), Some(changed_title.as_str()));
}

/// 没有自动订阅用户时，只写番剧元数据，不产生订阅。
#[tokio::test]
async fn sync_calendar_without_auto_sub_user_writes_no_subscription() {
    let app = TestApp::new().await;
    let users = app.users(Arc::new(MockDownloaderManager::new(true)));
    let user = users
        .create("manual-user", "test-password-123456", UserRole::User, false)
        .await
        .expect("create user failed");

    let captured = seasonal();
    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
    sync_calendar_task(app.animes(), source, users.clone(), app.sub_animes())
        .await
        .expect("sync_calendar_task failed");

    let list = app
        .animes()
        .list(anime::entity::model::AnimeListQuery {
            metadata_locked: None,
            keyword: None,
            year: None,
            month: None,
        })
        .await
        .expect("list anime failed");
    assert_eq!(list.len(), captured.len());

    let sub_list = app
        .sub_animes()
        .list(&SubAnimeListQuery {
            anime_id: None,
            space_id: Some(user.space_id()),
            search_status: None,
            sub_status: None,
            limit: None,
        })
        .await
        .expect("list sub anime failed");
    assert!(sub_list.is_empty());
}
