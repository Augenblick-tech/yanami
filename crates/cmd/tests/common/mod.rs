//! 功能测试公共夹具。
//!
//! 每个用例都构造真实 `AppContext` + 真实 SQLite（`tempfile::TempDir`，Drop 时自动清理），
//! 业务链路全部走生产代码；只有外部端口（来源抓取、访问策略、下载器、季节性数据源）用 mock 注入。
#![allow(dead_code)]

mod captured;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anime::entity::{
    anime_source::AnimeSources,
    animes::Animes,
    cap::{AnimeRepository, AnimeSeasonalProvider},
    model::{AnimeIdType, AnimeMetadata, AnimeSourceTarget},
};
use feed::entity::{
    cap::{FeedAccessPolicy, FeedFetcher, FeedRepository},
    feeds::Feeds,
    model::{FeedData, FeedFetchError, FeedFetchResult, FeedItem, FeedMetadata, FeedProp},
};
use futures::StreamExt;
use resource::entity::{
    cap::ResourceRepository,
    model::{ResourceBaseData, ResourceProp, ResourceQuery},
    resource_entity::ResourceEntity,
    resources::Resources,
};
use subscription::entity::{
    cap::{RuleRepository, SubAnimeRepository},
    model::{EpisodeProp, SubAnimeListQuery},
    search_mandates::SearchMandates,
    sub_anime_entity::SubAnimeEntity,
    sub_animes::SubAnimes,
};
use user::entity::{
    cap::{DownloadProvider, DownloaderManager, UserRepository},
    model::{
        DefaultDownloaderConfig, DownloadConfig, DownloadTask, DownloaderConfig, UserProps, UserRole,
    },
    users::Users,
};
use web::app_ctx::{AppContext, AuthConfig};

pub use captured::*;

/// 抓取到的当季条目 `転生したら剣でした`（TMDB 134667，TMDB 第 2 季），
/// 补一条真实发布名对应的韩文别名（抓取到的别名里没有这条正名）。
pub fn sword_anime() -> AnimeMetadata {
    let mut item = seasonal().into_iter().next().expect("captured data should not be empty");
    item.titles.push(anime::entity::model::AnimeTitle {
        name: "전생했더니 검이었습니다".to_string(),
        match_name: "전생했더니 검이었습니다".to_string(),
        target: anime::entity::model::AnimeLangTarget::KR,
        origin: false,
    });
    item
}

/// 把真实资源的集数换成 `episode`，同时改掉资源身份（url / info_hash）避免入库去重。
/// 只用于构造「同番剧多集」场景，标题与来源链接都取自真实抓取结果。
pub fn nyaa_item_with_episode(base: &FeedItem, episode: &str) -> FeedItem {
    let mut item = base.clone();
    item.title = item.title.replacen(" - 01 ", &format!(" - {episode} "), 1);
    assert!(
        item.title.contains(&format!(" - {episode} ")),
        "real item title should accept episode {episode}, actual {}",
        item.title
    );
    item.source_url = format!("{}#ep{episode}", item.source_url);
    item.resource_url = format!("{}#ep{episode}", item.resource_url);
    item.info_hash[0] = item
        .info_hash[0]
        .wrapping_add(episode.parse::<u8>().expect("episode number should be two decimal digits"));
    item
}

/// 测试用应用上下文：所有端口都是真实实现，数据库落在一次性临时目录里。
pub struct TestApp {
    pub ctx: Arc<AppContext>,
    // 声明顺序保证 `ctx`（连接池）先于临时目录析构
    dir: tempfile::TempDir,
}

impl TestApp {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir failed");
        let ctx = Arc::new(
            AppContext::new(
                "test.db",
                AuthConfig {
                    token: "test-token".to_string(),
                    expire: Duration::from_secs(3600),
                    crypto_secret: "yanami-test-crypto-secret".to_string(),
                },
                "test-tmdb-token".to_string(),
                dir.path().to_string_lossy().to_string(),
                Arc::new(|_| Ok(())),
            )
            .await,
        );
        ctx.init_database().await;
        Self { ctx, dir }
    }

    pub fn data_dir(&self) -> String {
        self.dir.path().to_string_lossy().to_string()
    }

    /// 生产 HTTP 路由（含鉴权中间件），功能测试用 `tower::ServiceExt::oneshot` 直连。
    pub fn router(&self) -> axum::Router {
        web::router::route(self.ctx.clone())
    }
}

// ---------- 领域集合装配：repo 用真实 sqlite client，外部端口用参数传入的 mock ----------

impl TestApp {
    pub fn animes(&self) -> Animes {
        Animes::new(self.ctx.repo.anime_repo.clone())
    }

    pub fn sub_animes(&self) -> SubAnimes {
        SubAnimes::new(
            self.ctx.repo.sub_anime_repo.clone(),
            self.ctx.repo.rule_repo.clone(),
            Arc::new(self.ctx.caps.matcher.clone()),
        )
    }

    pub fn feeds(&self, fetcher: Arc<dyn FeedFetcher>, policy: Arc<dyn FeedAccessPolicy>) -> Feeds {
        Feeds::new(self.ctx.repo.feed_repo.clone(), fetcher, policy)
    }

    pub fn resources(&self) -> Resources {
        Resources::new(self.ctx.repo.res_repo.clone())
    }

    pub fn users(&self, manager: Arc<dyn DownloaderManager>) -> Users {
        Users::new(
            self.ctx.repo.user_repo.clone(),
            manager,
            self.ctx.caps.crypto_provider.clone(),
        )
    }

    pub fn search_mandates(
        &self,
        fetcher: Arc<dyn FeedFetcher>,
        policy: Arc<dyn FeedAccessPolicy>,
    ) -> SearchMandates {
        SearchMandates::new(self.ctx.repo.mandate_repo.clone(), fetcher, policy)
    }

    /// 季节性数据源全部由 mock 注入；`sync()` 不使用 lookup provider，故传真实 bgm client（不会发请求）。
    pub fn anime_sources(&self, providers: Vec<Arc<dyn AnimeSeasonalProvider>>) -> AnimeSources {
        AnimeSources::new(self.ctx.caps.bgm_client.clone(), providers)
    }
}

// ---------- 造数：全部走生产代码的写入路径 ----------

impl TestApp {
    pub async fn seed_anime(&self, metadata: &AnimeMetadata) -> anime::entity::model::AnimeProps {
        self.ctx
            .repo
            .anime_repo
            .insert(metadata)
            .await
            .expect("seed anime failed")
    }

    pub async fn seed_user(
        &self,
        username: &str,
        role: UserRole,
        auto_sub: bool,
    ) -> UserProps {
        self.ctx
            .repo
            .user_repo
            .insert(username, "test-password-123456", role, auto_sub)
            .await
            .expect("seed user failed")
    }

    pub async fn seed_rule(&self, space_id: i64, name: &str, pattern: &str) -> i64 {
        let data = self
            .ctx
            .repo
            .rule_repo
            .insert(&subscription::entity::model::Rule {
                space_id,
                name: name.to_string(),
                order: 0,
                pattern: pattern.to_string(),
            })
            .await
            .expect("seed rule failed");
        data.id
    }

    pub async fn seed_feed(
        &self,
        title: &str,
        site_url: Option<&str>,
        search_url: Option<&str>,
    ) -> FeedProp {
        self.ctx
            .repo
            .feed_repo
            .insert(&FeedMetadata {
                title: title.to_string(),
                site_url: site_url.map(|i| i.to_string()),
                search_url: search_url.map(|i| i.to_string()),
                source_key: format!("test-source-key-{}", title),
            })
            .await
            .expect("seed feed failed")
    }

    pub async fn seed_resources(&self, items: Vec<ResourceBaseData>) -> Vec<ResourceProp> {
        self.ctx
            .repo
            .res_repo
            .insert_or_skip_return_new(items)
            .await
            .expect("seed resources failed")
    }
}

// ---------- 订阅/剧集/下载链路辅助：全部走生产方法推进状态 ----------

impl TestApp {
    /// 新建订阅并推进到 NotSearch：资源任务只匹配 NotSearch + Enable 的订阅，
    /// 而 `insert_sub_anime` 建出来的订阅是 Pending，这里用生产状态机做这一步。
    pub async fn subscribe(&self, space_id: i64, anime_id: i64) -> SubAnimeEntity {
        let sub_animes = self.sub_animes();
        let mut sub_anime = sub_animes
            .create(space_id, anime_id)
            .await
            .expect("create sub anime failed");
        assert!(sub_anime.cancel_search(), "fresh subscription should return to NotSearch from Pending");
        sub_animes
            .save(&sub_anime)
            .await
            .expect("save sub anime failed");
        sub_anime
    }

    /// 走真实匹配链路把库里已有的资源匹配进订阅并落库（与 `get_resource_and_match_task` 同一套方法）。
    pub async fn match_all_resources(&self, sub_anime: &SubAnimeEntity) {
        let sub_animes = self.sub_animes();
        let mut matcher = sub_animes
            .as_matcher(sub_anime)
            .await
            .expect("as matcher failed");

        let resources = self.resources();
        let query = ResourceQuery {
            keywords: None,
            start_at: None,
            end_at: None,
            limit: None,
            offset: None,
        };
        let mut stream = resources.stream(&query);
        while let Some(item) = stream.next().await {
            let resource = item.expect("stream resource failed");
            matcher
                .match_resource(&resource)
                .expect("match resource failed");
        }
        drop(stream);

        sub_animes
            .save_matcher(&matcher)
            .await
            .expect("save matcher failed");
    }

    /// 通过生产抓取结果写入资源（`Resources::save` 是拿到 `ResourceEntity` 的唯一入口）。
    pub async fn save_feed_items(&self, items: Vec<FeedItem>) -> Vec<ResourceEntity> {
        self.resources()
            .save(items)
            .await
            .expect("save resources failed")
    }

    pub async fn list_eps(&self, sub_anime_id: i64) -> Vec<EpisodeProp> {
        self.ctx
            .repo
            .sub_anime_repo
            .list_eps(sub_anime_id)
            .await
            .expect("list eps failed")
    }

    pub async fn list_sub_animes(&self, query: &SubAnimeListQuery) -> Vec<subscription::entity::model::SubAnimeProps> {
        self.ctx
            .repo
            .sub_anime_repo
            .list(query)
            .await
            .expect("list sub animes failed")
    }

    /// 给用户配一个启用的默认下载器，走生产的加密存储与启用校验。
    pub async fn enable_default_downloader(&self, users: &Users, username: &str, base_path: &str) {
        let mut user = users
            .get_by_username(username)
            .await
            .expect("find user failed")
            .expect("user missing");
        user.save_download_config(DownloaderConfig::Default(DownloadConfig {
            name: "default".to_string(),
            active: false,
            base_path: base_path.to_string(),
            config: DefaultDownloaderConfig {
                max_seed_time: None,
                max_seed_ratio: None,
                max_upload_speed: None,
            },
        }))
        .expect("save download config failed");
        user.enable_download_config("default")
            .expect("enable download config failed");
        users.save(&user).await.expect("save user failed");
    }
}

// ---------- 外部端口 mock ----------

/// 抓取端口 mock：按 url 返回预先硬编码的真实抓取结果，并记录被请求过的 url。
pub struct MockFeedFetcher {
    responses: Mutex<HashMap<String, FeedData>>,
    // 未注册 url 时的兜底结果（搜索委托的 url 由关键词拼出来，数量不定）
    any_feed: Mutex<Option<FeedData>>,
    any_failure: bool,
    requested: Mutex<Vec<String>>,
}

impl Default for MockFeedFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFeedFetcher {
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            any_feed: Mutex::new(None),
            any_failure: false,
            requested: Mutex::new(Vec::new()),
        }
    }

    /// 注册某个 url 的真实抓取结果。
    pub fn with_feed(self, url: &str, data: FeedData) -> Self {
        self.responses
            .lock()
            .expect("lock responses failed")
            .insert(url.to_string(), data);
        self
    }

    /// 未注册的 url 一律返回这份真实抓取结果。
    pub fn with_any_feed(self, data: FeedData) -> Self {
        *self.any_feed.lock().expect("lock any feed failed") = Some(data);
        self
    }

    /// 任何 url 都抓取失败（网络不可达）。
    pub fn with_any_failure(self) -> Self {
        Self {
            any_failure: true,
            ..self
        }
    }

    pub fn requested(&self) -> Vec<String> {
        self.requested
            .lock()
            .expect("lock requested failed")
            .clone()
    }
}

#[async_trait::async_trait]
impl FeedFetcher for MockFeedFetcher {
    async fn fetch_url(&self, url: &str) -> Result<FeedData, FeedFetchError> {
        self.requested
            .lock()
            .expect("lock requested failed")
            .push(url.to_string());
        if self.any_failure {
            return Err(FeedFetchError::Inaccessible(format!(
                "mock feed is unreachable: {}",
                url
            )));
        }
        let guard = self.responses.lock().expect("lock responses failed");
        if let Some(data) = guard.get(url) {
            return Ok(data.clone());
        }
        drop(guard);
        if let Some(data) = self
            .any_feed
            .lock()
            .expect("lock any feed failed")
            .as_ref()
        {
            return Ok(data.clone());
        }
        Err(FeedFetchError::Inaccessible(format!(
            "mock has no captured feed for {}",
            url
        )))
    }

    async fn get_source_key(&self, url: &str) -> Result<String, FeedFetchError> {
        Ok(format!("mock-source-key-{}", url))
    }
}

/// 访问策略 mock：可选择放行/拦截，并记录 `note` 被调用的 feed id。
pub struct MockAccessPolicy {
    allow: bool,
    noted: Mutex<Vec<i64>>,
}

impl MockAccessPolicy {
    pub fn allow() -> Self {
        Self {
            allow: true,
            noted: Mutex::new(Vec::new()),
        }
    }

    pub fn deny() -> Self {
        Self {
            allow: false,
            noted: Mutex::new(Vec::new()),
        }
    }

    pub fn noted(&self) -> Vec<i64> {
        self.noted.lock().expect("lock noted failed").clone()
    }
}

impl FeedAccessPolicy for MockAccessPolicy {
    fn block_feed_ids(&self) -> Vec<i64> {
        vec![]
    }

    fn block_feed_details(&self) -> Vec<(i64, i64)> {
        vec![]
    }

    fn is_access(&self, _feed_id: i64) -> bool {
        self.allow
    }

    fn note(&self, feed_id: i64, _res: &FeedFetchResult) {
        self.noted
            .lock()
            .expect("lock noted failed")
            .push(feed_id);
    }
}

/// 下载器 mock：记录 `download` 收到的 (url, path, hash)，返回值可配置。
pub struct MockDownloadProvider {
    downloads: Mutex<Vec<(String, String, [u8; 20])>>,
    result: bool,
}

impl MockDownloadProvider {
    pub fn new(result: bool) -> Self {
        Self {
            downloads: Mutex::new(Vec::new()),
            result,
        }
    }

    pub fn downloads(&self) -> Vec<(String, String, [u8; 20])> {
        self.downloads
            .lock()
            .expect("lock downloads failed")
            .clone()
    }
}

#[async_trait::async_trait]
impl DownloadProvider for MockDownloadProvider {
    fn name(&self) -> &str {
        "mock-downloader"
    }

    async fn stop(&self) {}

    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> anyhow::Result<bool> {
        self.downloads
            .lock()
            .expect("lock downloads failed")
            .push((url.to_string(), path.to_string(), hash));
        Ok(self.result)
    }

    async fn list_task(&self) -> anyhow::Result<Vec<DownloadTask>> {
        Ok(vec![])
    }

    async fn get_task(&self, _hash: [u8; 20]) -> anyhow::Result<Option<DownloadTask>> {
        Ok(None)
    }

    async fn pause_task(&self, _hash: [u8; 20]) -> anyhow::Result<()> {
        Ok(())
    }

    async fn resume_task(&self, _hash: [u8; 20]) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete_task(&self, _hash: [u8; 20]) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct MockDownloaderManager {
    provider: Arc<MockDownloadProvider>,
}

impl MockDownloaderManager {
    pub fn new(result: bool) -> Self {
        Self {
            provider: Arc::new(MockDownloadProvider::new(result)),
        }
    }

    pub fn provider(&self) -> Arc<MockDownloadProvider> {
        self.provider.clone()
    }
}

#[async_trait::async_trait]
impl DownloaderManager for MockDownloaderManager {
    async fn get(
        &self,
        _user_id: i64,
        _config: &DownloaderConfig,
    ) -> anyhow::Result<Arc<dyn DownloadProvider>> {
        Ok(self.provider.clone())
    }

    async fn validate_config(&self, _config: &DownloaderConfig) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 季节性数据源 mock：返回硬编码的真实抓取结果。
pub struct MockSeasonalProvider {
    data: Vec<AnimeMetadata>,
}

impl MockSeasonalProvider {
    pub fn new(data: Vec<AnimeMetadata>) -> Self {
        Self { data }
    }
}

#[async_trait::async_trait]
impl AnimeSeasonalProvider for MockSeasonalProvider {
    async fn get(&self) -> anyhow::Result<Vec<AnimeMetadata>> {
        Ok(self.data.clone())
    }

    fn name(&self) -> &str {
        "mock-seasonal"
    }
}

// ---------- 断言辅助 ----------

/// 取条目的 TMDB 外部身份（系列身份的唯一来源）。
pub fn tmdb_id_of(item: &AnimeMetadata) -> Option<i64> {
    item.external_link.iter().find_map(|link| match (&link.target, &link.id) {
        (AnimeSourceTarget::TMDB, AnimeIdType::Int(id)) => Some(*id),
        _ => None,
    })
}

/// 取条目的 Bangumi 外部身份。
pub fn bangumi_id_of(item: &AnimeMetadata) -> Option<i64> {
    item.external_link.iter().find_map(|link| match (&link.target, &link.id) {
        (AnimeSourceTarget::Bangumi, AnimeIdType::Int(id)) => Some(*id),
        _ => None,
    })
}
