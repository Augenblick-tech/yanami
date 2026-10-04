//! 功能测试：`sync_calendar_task` 同步链路（来源数据 → 番剧元数据 → 系列 → 用户自动订阅）。

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anime::entity::cap::AnimeRepository;
use async_trait::async_trait;
use cmd::task::sync_calendar_task::sync_calendar_task;
use common::{
    MockDownloaderManager, MockSeasonalProvider, TestApp, bangumi_id_of, seasonal, tmdb_id_of,
};
use subscription::entity::cap::SubAnimeRepository;
use subscription::entity::model::{
    Episode, EpisodeBaseData, EpisodeProp, SubAnimeBaseData, SubAnimeListQuery, SubAnimeProps,
};
use subscription::entity::sub_animes::SubAnimes;
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

/// 统计订阅写入次数的假仓储：同步链路只用 `insert_sub_anime` 与 `find_by_anime_ids`。
struct CountingSubAnimeRepository {
    inner: Arc<dyn SubAnimeRepository>,
    inserts: AtomicUsize,
}

impl CountingSubAnimeRepository {
    fn new(inner: Arc<dyn SubAnimeRepository>) -> Self {
        Self {
            inner,
            inserts: AtomicUsize::new(0),
        }
    }

    fn inserts(&self) -> usize {
        self.inserts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SubAnimeRepository for CountingSubAnimeRepository {
    async fn insert_sub_anime(
        &self,
        space_id: i64,
        anime_id: i64,
    ) -> anyhow::Result<SubAnimeProps> {
        self.inserts.fetch_add(1, Ordering::SeqCst);
        self.inner.insert_sub_anime(space_id, anime_id).await
    }

    async fn find_by_anime_ids(
        &self,
        space_id: i64,
        anime_ids: Vec<i64>,
    ) -> anyhow::Result<Vec<SubAnimeProps>> {
        self.inner.find_by_anime_ids(space_id, anime_ids).await
    }

    async fn update_sub_anime(&self, _data: &SubAnimeBaseData) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not update sub anime")
    }

    async fn update_sub_animes(&self, _data: &[SubAnimeBaseData]) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not update sub animes")
    }

    async fn find_sub_anime(&self, _id: i64) -> anyhow::Result<Option<SubAnimeProps>> {
        unimplemented!("sync calendar task must not find sub anime by id")
    }

    async fn list(&self, _query: &SubAnimeListQuery) -> anyhow::Result<Vec<SubAnimeProps>> {
        unimplemented!("sync calendar task must not list sub animes")
    }

    async fn list_by_mandate(&self, _anime_id: i64) -> anyhow::Result<Vec<SubAnimeProps>> {
        unimplemented!("sync calendar task must not list sub animes by mandate")
    }

    async fn list_eps(&self, _sub_anime_id: i64) -> anyhow::Result<Vec<EpisodeProp>> {
        unimplemented!("sync calendar task must not list episodes")
    }

    async fn find_epsiode(&self, _ep_id: i64) -> anyhow::Result<Option<EpisodeProp>> {
        unimplemented!("sync calendar task must not find episode")
    }

    async fn get_one_undownload_ep(&self) -> anyhow::Result<Option<EpisodeProp>> {
        unimplemented!("sync calendar task must not get one undownload episode")
    }

    async fn update_epsiode_status(&self, _data: &EpisodeBaseData) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not update episode status")
    }

    async fn update_epsiodes_status(&self, _data: &[EpisodeBaseData]) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not update episodes status")
    }

    async fn update_sub_anime_progress(
        &self,
        _data: &SubAnimeBaseData,
        _eps: &[Episode],
    ) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not update sub anime progress")
    }

    async fn delete(&self, _sub_anime: i64) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not delete sub anime")
    }

    async fn binding_rule_and_clear_eps(
        &self,
        _sub_anime: i64,
        _rule_id: i64,
    ) -> anyhow::Result<()> {
        unimplemented!("sync calendar task must not bind rule")
    }
}

/// 已订阅的番剧不再重复订阅：第二次同步不再产生任何订阅写入，既有订阅保持不变。
#[tokio::test]
async fn sync_calendar_skips_already_subscribed_animes() {
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

    let repo = Arc::new(CountingSubAnimeRepository::new(
        app.ctx.repo.sub_anime_repo.clone(),
    ));
    let sub_animes = SubAnimes::new(
        repo.clone(),
        app.ctx.repo.rule_repo.clone(),
        Arc::new(app.ctx.caps.matcher.clone()),
    );

    // 第一次同步：每个自动订阅用户的每条番剧各写一次订阅
    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
    sync_calendar_task(app.animes(), source, users.clone(), sub_animes.clone())
        .await
        .expect("first sync failed");
    let auto_sub_user_count = users
        .list_auto_sub()
        .await
        .expect("list auto sub failed")
        .len();
    let expected_inserts = captured.len() * auto_sub_user_count;
    assert_eq!(
        repo.inserts(),
        expected_inserts,
        "first sync must subscribe every anime for every auto sub user"
    );

    // 手工改动一条既有订阅（绑定规则），同步不允许把它覆盖回初始状态
    let query = SubAnimeListQuery {
        anime_id: None,
        space_id: Some(user.space_id()),
        search_status: None,
        sub_status: None,
        limit: None,
    };
    let rule_id = app
        .seed_rule(user.space_id(), "auto-sub-rule", "1080")
        .await;
    let subscribed = app
        .list_sub_animes(&query)
        .await
        .into_iter()
        .next()
        .expect("first sync must write subscriptions");
    app.ctx
        .repo
        .sub_anime_repo
        .binding_rule_and_clear_eps(subscribed.data.id, rule_id)
        .await
        .expect("binding rule failed");

    // 第二次同步：已订阅的番剧直接跳过，不再写入
    let source = app.anime_sources(vec![Arc::new(MockSeasonalProvider::new(captured.clone()))]);
    sync_calendar_task(app.animes(), source, users.clone(), sub_animes.clone())
        .await
        .expect("second sync failed");
    assert_eq!(
        repo.inserts(),
        expected_inserts,
        "already subscribed animes must not be inserted again"
    );

    let sub_list = app
        .sub_animes()
        .list(&query)
        .await
        .expect("list sub anime failed");
    assert_eq!(
        sub_list.len(),
        captured.len(),
        "subscriptions must not duplicate"
    );

    let after = app
        .list_sub_animes(&query)
        .await
        .into_iter()
        .find(|i| i.data.id == subscribed.data.id)
        .expect("subscribed anime must still exist");
    assert_eq!(
        after.data.rule_id,
        Some(rule_id),
        "existing subscription must be kept untouched"
    );
}
