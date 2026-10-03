//! 功能测试：下载链路（订阅剧集 → 系列落点 → 下载器），来源数据取自真实抓取结果。
//!
//! 番剧元数据用 `seasonal()` 里抓到的 `転生したら剣でした`（TMDB 134667，TMDB 第 2 季），
//! 资源用 nyaa 真实条目 `[Doomdos] - 전생했더니 검이었습니다 Ⅱ - 01 ...`
//! （韩文正名，与同一条目同番剧；抓取到的别名里没有这条正名，故测试里补一条 KR 标题）。

mod common;

use std::sync::Arc;

use anime::entity::model::AnimeMetadata;
use cmd::task::download_task::download_task;
use common::{MockDownloaderManager, TestApp, nyaa_item, sword_anime};
use subscription::entity::model::EpsiodeStatus;
use user::entity::model::UserRole;

/// 订阅 + 匹配出一条剧集，返回 (订阅 id, 匹配到的资源 url, 资源 info_hash)。
async fn arrange_one_episode(app: &TestApp, metadata: &AnimeMetadata) -> (i64, String, [u8; 20]) {
    let user = app.seed_user("dl-user", UserRole::User, false).await;
    let space_id = user.data.space_id;

    let anime = app.seed_anime(metadata).await;
    app.seed_rule(space_id, "kr-rule", "전생했더니 검이었습니다")
        .await;

    let sub_anime = app.subscribe(space_id, anime.data.id).await;

    let item = nyaa_item("전생했더니");
    assert!(
        item.title.contains("01"),
        "real item title should contain an episode number, actual {}",
        item.title
    );
    let resource_url = item.resource_url.clone();
    let info_hash = item.info_hash;

    let saved = app.save_feed_items(vec![item]).await;
    assert_eq!(
        saved.len(),
        1,
        "real item should be saved into the resource table"
    );

    app.match_all_resources(&sub_anime).await;
    let eps = app.list_eps(sub_anime.id()).await;
    assert_eq!(eps.len(), 1, "only one episode should be matched");
    assert_eq!(
        eps[0].data.ep.ep_num,
        Some(1.0),
        "episode number should be extracted from 01 in the title"
    );

    (sub_anime.id(), resource_url, info_hash)
}

/// 下载成功：落点是「系列目录 / S{季号}」，剧集状态推进为已下载。
#[tokio::test]
async fn download_task_lands_episode_in_series_season_folder() {
    let app = TestApp::new().await;
    let manager = Arc::new(MockDownloaderManager::new(true));
    let users = app.users(manager.clone());

    let (sub_anime_id, resource_url, info_hash) = arrange_one_episode(&app, &sword_anime()).await;

    let base_path = format!("{}/downloads", app.data_dir());
    app.enable_default_downloader(&users, "dl-user", &base_path)
        .await;

    download_task(app.sub_animes(), users.clone(), app.animes())
        .await
        .expect("download_task failed");

    // 系列展示信息来自抓取的 TMDB 系列，季号来自同一条目的 TMDB 季
    let expected_path = format!("{}/転生したら剣でした/S02", base_path);
    let downloads = manager.provider().downloads();
    assert_eq!(
        downloads.len(),
        1,
        "download should be triggered exactly once"
    );
    assert_eq!(downloads[0].0, resource_url);
    assert_eq!(downloads[0].1, expected_path);
    assert_eq!(downloads[0].2, info_hash);

    let eps = app.list_eps(sub_anime_id).await;
    assert_eq!(eps[0].data.ep.status, EpsiodeStatus::Downloaded);
}

/// 系列展示信息缺失（有 TMDB 身份但没有 `anime_series` 行）：落点算不出来，任务返回错误，
/// 剧集保持待下载，下一轮会被重新取到 —— 不会丢状态，也不会伪造落点。
///
/// 已知缺陷：`download_task` 里「系列取不到就记日志放回队列」的分支只兜住了「查不到系列实体」，
/// 而落点解析是在 `EpsiodeEntity::download` 内部做的，系列行缺失时错误会直接冒泡成任务错误
/// （注释与行为不一致；状态机本身不受影响）。
#[tokio::test]
async fn download_task_reports_error_when_series_row_missing() {
    let app = TestApp::new().await;
    let manager = Arc::new(MockDownloaderManager::new(true));
    let users = app.users(manager.clone());

    // 保留 TMDB 外部身份（系列身份来源），但不带系列展示信息，anime_series 里就没有行
    let mut metadata = sword_anime();
    metadata.series_metadata = None;
    let (sub_anime_id, _, _) = arrange_one_episode(&app, &metadata).await;

    let base_path = format!("{}/downloads", app.data_dir());
    app.enable_default_downloader(&users, "dl-user", &base_path)
        .await;

    let error = download_task(app.sub_animes(), users.clone(), app.animes())
        .await
        .expect_err("location resolve should fail when series metadata is unavailable");
    assert!(
        error
            .to_string()
            .contains("anime series metadata not found"),
        "actual error: {error}"
    );

    assert!(
        manager.provider().downloads().is_empty(),
        "downloader must not be called when location cannot be resolved"
    );
    let eps = app.list_eps(sub_anime_id).await;
    assert_eq!(eps[0].data.ep.status, EpsiodeStatus::Pending);
}

/// 下载器返回失败：剧集保持待下载，不会被记成已下载。
#[tokio::test]
async fn download_task_keeps_episode_pending_when_downloader_fails() {
    let app = TestApp::new().await;
    let manager = Arc::new(MockDownloaderManager::new(false));
    let users = app.users(manager.clone());

    let (sub_anime_id, _, _) = arrange_one_episode(&app, &sword_anime()).await;

    let base_path = format!("{}/downloads", app.data_dir());
    app.enable_default_downloader(&users, "dl-user", &base_path)
        .await;

    download_task(app.sub_animes(), users.clone(), app.animes())
        .await
        .expect("download_task failed");

    assert_eq!(
        manager.provider().downloads().len(),
        1,
        "downloader should have been called"
    );
    let eps = app.list_eps(sub_anime_id).await;
    assert_eq!(eps[0].data.ep.status, EpsiodeStatus::Pending);
}

/// 用户没有启用的下载器：任务直接跳过，不产生下载。
#[tokio::test]
async fn download_task_skips_episode_when_user_has_no_active_downloader() {
    let app = TestApp::new().await;
    let manager = Arc::new(MockDownloaderManager::new(true));
    let users = app.users(manager.clone());

    let (sub_anime_id, _, _) = arrange_one_episode(&app, &sword_anime()).await;

    download_task(app.sub_animes(), users.clone(), app.animes())
        .await
        .expect("download_task failed");

    assert!(manager.provider().downloads().is_empty());
    let eps = app.list_eps(sub_anime_id).await;
    assert_eq!(eps[0].data.ep.status, EpsiodeStatus::Pending);
}
