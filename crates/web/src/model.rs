use anime::entity::model::{
    AnimeAirWeekday, AnimeEpisode, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeMetadata,
    AnimeSearchResult, AnimeSeason, AnimeSourceTarget, AnimeTitle,
};
use chrono::NaiveDate;
use feed::entity::feed_entity::FeedEntity;
use serde::{Deserialize, Serialize};
use subscription::entity::episode_entity::EpsiodeEntity;
use subscription::entity::rule_entity::RuleEntity;
use user::entity::model::{DownloadConfig, DownloaderConfig, QbitConfig, UserRole};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogLevelRequest {
    /// 日志级别，如 "info", "debug", "trace" 或更复杂的 EnvFilter 字符串
    #[schema(example = "debug")]
    pub level: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// 用户名
    #[schema(example = "admin")]
    pub username: String,
    /// 明文密码
    #[schema(example = "your_password")]
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// 旧密码
    #[schema(example = "old_password")]
    pub old_password: String,
    /// 新密码
    #[schema(example = "new_password")]
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AutoSubRequest {
    /// 是否开启自动订阅
    #[schema(example = true)]
    pub auto_sub: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AutoSubResponse {
    /// 是否开启自动订阅
    #[schema(example = true)]
    pub auto_sub: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSubscriptionRequest {
    /// 番剧ID
    #[schema(example = 1)]
    pub anime_id: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PageAnimeRequest {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub keyword: Option<String>,
    pub lang: Option<String>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub subscription: Option<bool>,
    pub search_status: Option<i64>,
    // 订阅状态
    pub status: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AnimeResponse {
    pub id: i64,
    pub name: String,
    pub name_target: Option<String>,
    pub desc: String,
    pub air_date: String,
    pub air_weekday: i64,
    pub eps: u32,
    pub sub_info: Option<AnimeSubInfo>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AnimeSubInfo {
    pub sub_anime_id: i64,
    pub search_status: i32,
    pub progress: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    pub page: usize,
    pub page_size: usize,
    pub total: u64,
    pub data: T,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    /// 业务码，200 表示成功
    pub code: i64,
    /// 响应数据
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 200, data }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// 用户标识
    pub user_id: i64,
    /// 用户角色：admin / user
    pub role: u8,
    /// JWT 访问令牌
    pub access_token: String,
    /// 令牌类型
    pub token_type: String,
    /// 令牌过期时间戳（秒）
    pub expires_at: i64,
}

impl From<LoginOutcome> for LoginResponse {
    fn from(outcome: LoginOutcome) -> Self {
        Self {
            user_id: outcome.user_id,
            role: outcome.role.into(),
            access_token: outcome.access_token.access_token,
            token_type: outcome.access_token.token_type,
            expires_at: outcome.access_token.expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub user_id: i64,
    pub exp: usize,
    pub character: UserRole,
}

#[derive(Debug, Clone)]
pub struct LoginOutcome {
    pub user_id: i64,
    pub role: UserRole,
    pub access_token: AccessToken,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeedItemRequest {
    /// 名字
    #[schema(example = "dmhy")]
    pub title: String,
    /// RSS地址
    #[schema(example = "https://example.com")]
    pub site_url: Option<String>,
    /// RSS搜索地址
    #[schema(example = "https://example.com?keyword={}")]
    pub search_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeedItem {
    /// ID
    #[schema(example = 1)]
    pub id: i64,
    /// 名字
    #[schema(example = "dmhy")]
    pub title: String,
    /// RSS地址
    #[schema(example = "https://example.com")]
    pub site_url: Option<String>,
    /// RSS搜索地址
    #[schema(example = "https://example.com?keyword={}")]
    pub search_url: Option<String>,
}

impl From<FeedEntity> for FeedItem {
    fn from(value: FeedEntity) -> Self {
        Self {
            id: value.id(),
            title: value.title().to_string(),
            site_url: value.site_url().map(String::from),
            search_url: value.search_url().map(String::from),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct QbitSettings {
    pub username: String,
    pub password: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct DownloadSettings<T> {
    pub name: String,
    pub active: bool,
    pub base_path: String,
    pub config: T,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub enum DownloaderSettings {
    Qbit(DownloadSettings<QbitSettings>),
}
impl From<QbitConfig> for QbitSettings {
    fn from(config: QbitConfig) -> Self {
        Self {
            username: config.username,
            password: config.password,
            url: config.url,
        }
    }
}

impl From<QbitSettings> for QbitConfig {
    fn from(settings: QbitSettings) -> Self {
        Self {
            username: settings.username,
            password: settings.password,
            url: settings.url,
        }
    }
}

impl From<DownloadConfig<QbitConfig>> for DownloadSettings<QbitSettings> {
    fn from(config: DownloadConfig<QbitConfig>) -> Self {
        Self {
            name: config.name,
            active: config.active,
            base_path: config.base_path,
            config: config.config.into(),
        }
    }
}

impl From<DownloadSettings<QbitSettings>> for DownloadConfig<QbitConfig> {
    fn from(settings: DownloadSettings<QbitSettings>) -> Self {
        Self {
            name: settings.name,
            active: settings.active,
            base_path: settings.base_path,
            config: settings.config.into(),
        }
    }
}

impl From<&DownloaderConfig> for DownloaderSettings {
    fn from(config: &DownloaderConfig) -> Self {
        config.clone().into()
    }
}

impl From<DownloaderConfig> for DownloaderSettings {
    fn from(config: DownloaderConfig) -> Self {
        match config {
            DownloaderConfig::Qbit(c) => DownloaderSettings::Qbit(c.into()),
        }
    }
}

impl From<DownloaderSettings> for DownloaderConfig {
    fn from(settings: DownloaderSettings) -> Self {
        match settings {
            DownloaderSettings::Qbit(c) => DownloaderConfig::Qbit(c.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuleCreateRequest {
    /// 规则名称
    #[schema(example = "规则1")]
    pub name: String,
    /// 匹配正则表达式
    #[schema(example = ".*")]
    pub pattern: String,
    /// 优先级序号，值越小优先级越高
    #[schema(example = 10)]
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuleUpdateOrderRequest {
    /// 优先级序号，值越小优先级越高
    #[schema(example = 10)]
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuleItem {
    pub id: i64,
    pub name: String,
    pub order: i64,
    pub pattern: String,
}

impl From<RuleEntity> for RuleItem {
    fn from(value: RuleEntity) -> Self {
        Self {
            id: value.id(),
            name: value.name().to_string(),
            order: value.order(),
            pattern: value.pattern().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EpisodeItem {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub status: i32,
    pub ep_num: Option<f64>,
}

impl From<EpsiodeEntity> for EpisodeItem {
    fn from(value: EpsiodeEntity) -> Self {
        Self {
            id: value.id(),
            title: value.title().to_string(),
            url: value.url().to_string(),
            status: value.status().into(),
            ep_num: value.ep_num(),
        }
    }
}

/// 搜索番剧请求参数
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SearchAnimeQuery {
    /// 搜索关键字
    pub keyword: String,
}

/// 创建番剧请求
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateAnimeRequest {
    /// 番剧完整元数据
    pub metadata: AnimeMetadataItem,
    /// 是否锁定元数据 (锁定后自动任务将不再覆盖这些数据)
    pub lock: bool,
}

/// 番剧搜索结果项
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchAnimeItem {
    /// Bangumi 内部 ID
    pub id: i64,
    /// 原始名称
    pub name: String,
    /// 中文名称 (可能为空)
    pub name_cn: Option<String>,
}

impl From<AnimeSearchResult> for SearchAnimeItem {
    fn from(value: AnimeSearchResult) -> Self {
        Self {
            id: value.id,
            name: value.name,
            name_cn: value.name_cn,
        }
    }
}

/// 放送星期几
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum AnimeAirWeekdayItem {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<AnimeAirWeekday> for AnimeAirWeekdayItem {
    fn from(v: AnimeAirWeekday) -> Self {
        match v {
            AnimeAirWeekday::Monday => AnimeAirWeekdayItem::Monday,
            AnimeAirWeekday::Tuesday => AnimeAirWeekdayItem::Tuesday,
            AnimeAirWeekday::Wednesday => AnimeAirWeekdayItem::Wednesday,
            AnimeAirWeekday::Thursday => AnimeAirWeekdayItem::Thursday,
            AnimeAirWeekday::Friday => AnimeAirWeekdayItem::Friday,
            AnimeAirWeekday::Saturday => AnimeAirWeekdayItem::Saturday,
            AnimeAirWeekday::Sunday => AnimeAirWeekdayItem::Sunday,
        }
    }
}

impl From<AnimeAirWeekdayItem> for AnimeAirWeekday {
    fn from(v: AnimeAirWeekdayItem) -> Self {
        match v {
            AnimeAirWeekdayItem::Monday => AnimeAirWeekday::Monday,
            AnimeAirWeekdayItem::Tuesday => AnimeAirWeekday::Tuesday,
            AnimeAirWeekdayItem::Wednesday => AnimeAirWeekday::Wednesday,
            AnimeAirWeekdayItem::Thursday => AnimeAirWeekday::Thursday,
            AnimeAirWeekdayItem::Friday => AnimeAirWeekday::Friday,
            AnimeAirWeekdayItem::Saturday => AnimeAirWeekday::Saturday,
            AnimeAirWeekdayItem::Sunday => AnimeAirWeekday::Sunday,
        }
    }
}

/// 语言目标
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum AnimeLangTargetItem {
    JP,
    ZhCn,
    ZhTw,
    EN,
    KR,
    Other(String),
}

impl From<AnimeLangTarget> for AnimeLangTargetItem {
    fn from(v: AnimeLangTarget) -> Self {
        match v {
            AnimeLangTarget::JP => AnimeLangTargetItem::JP,
            AnimeLangTarget::ZhCn => AnimeLangTargetItem::ZhCn,
            AnimeLangTarget::ZhTw => AnimeLangTargetItem::ZhTw,
            AnimeLangTarget::EN => AnimeLangTargetItem::EN,
            AnimeLangTarget::KR => AnimeLangTargetItem::KR,
            AnimeLangTarget::Other(s) => AnimeLangTargetItem::Other(s),
        }
    }
}

impl From<AnimeLangTargetItem> for AnimeLangTarget {
    fn from(v: AnimeLangTargetItem) -> Self {
        match v {
            AnimeLangTargetItem::JP => AnimeLangTarget::JP,
            AnimeLangTargetItem::ZhCn => AnimeLangTarget::ZhCn,
            AnimeLangTargetItem::ZhTw => AnimeLangTarget::ZhTw,
            AnimeLangTargetItem::EN => AnimeLangTarget::EN,
            AnimeLangTargetItem::KR => AnimeLangTarget::KR,
            AnimeLangTargetItem::Other(s) => AnimeLangTarget::Other(s),
        }
    }
}

/// 番剧ID类型
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum AnimeIdTypeItem {
    Int(i64),
    String(String),
}

impl From<AnimeIdType> for AnimeIdTypeItem {
    fn from(v: AnimeIdType) -> Self {
        match v {
            AnimeIdType::Int(i) => AnimeIdTypeItem::Int(i),
            AnimeIdType::String(s) => AnimeIdTypeItem::String(s),
        }
    }
}

impl From<AnimeIdTypeItem> for AnimeIdType {
    fn from(v: AnimeIdTypeItem) -> Self {
        match v {
            AnimeIdTypeItem::Int(i) => AnimeIdType::Int(i),
            AnimeIdTypeItem::String(s) => AnimeIdType::String(s),
        }
    }
}

/// 来源平台
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum AnimeSourceTargetItem {
    #[allow(clippy::upper_case_acronyms)]
    TMDB,
    Bangumi,
    Other(String),
}

impl From<AnimeSourceTarget> for AnimeSourceTargetItem {
    fn from(v: AnimeSourceTarget) -> Self {
        match v {
            AnimeSourceTarget::TMDB => AnimeSourceTargetItem::TMDB,
            AnimeSourceTarget::Bangumi => AnimeSourceTargetItem::Bangumi,
            AnimeSourceTarget::Other(s) => AnimeSourceTargetItem::Other(s),
        }
    }
}

impl From<AnimeSourceTargetItem> for AnimeSourceTarget {
    fn from(v: AnimeSourceTargetItem) -> Self {
        match v {
            AnimeSourceTargetItem::TMDB => AnimeSourceTarget::TMDB,
            AnimeSourceTargetItem::Bangumi => AnimeSourceTarget::Bangumi,
            AnimeSourceTargetItem::Other(s) => AnimeSourceTarget::Other(s),
        }
    }
}

/// 标题信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeTitleItem {
    /// 标题名称
    pub name: String,
    /// 用于匹配和搜索的标准化名称
    pub match_name: String,
    /// 语言
    pub target: AnimeLangTargetItem,
    /// 是否是原名
    pub origin: bool,
}

impl From<AnimeTitle> for AnimeTitleItem {
    fn from(v: AnimeTitle) -> Self {
        Self {
            name: v.name,
            match_name: v.match_name,
            target: v.target.into(),
            origin: v.origin,
        }
    }
}

impl From<AnimeTitleItem> for AnimeTitle {
    fn from(v: AnimeTitleItem) -> Self {
        Self {
            name: v.name,
            match_name: v.match_name,
            target: v.target.into(),
            origin: v.origin,
        }
    }
}

/// 外部链接/关联ID
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeExItem {
    /// 外部ID
    pub id: AnimeIdTypeItem,
    /// 来源平台
    pub target: AnimeSourceTargetItem,
    /// 类型(可选)
    pub r#type: Option<String>,
}

impl From<AnimeEx> for AnimeExItem {
    fn from(v: AnimeEx) -> Self {
        Self {
            id: v.id.into(),
            target: v.target.into(),
            r#type: v.r#type,
        }
    }
}

impl From<AnimeExItem> for AnimeEx {
    fn from(v: AnimeExItem) -> Self {
        Self {
            id: v.id.into(),
            target: v.target.into(),
            r#type: v.r#type,
        }
    }
}

/// 剧集信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeEpisodeItem {
    /// 剧集编号 (绝对编号或相对编号)
    pub ep: u32,
    /// 用于排序的编号
    pub sort: f64,
    /// 放送日期
    pub air_date: NaiveDate,
    /// 剧集多语言标题
    pub title: Vec<AnimeTitleItem>,
    /// 时长(秒)
    pub duration_seconds: u64,
    /// 剧集简介
    pub desc: String,
    /// 外部ID
    pub ex_id: AnimeIdTypeItem,
}

impl From<AnimeEpisode> for AnimeEpisodeItem {
    fn from(v: AnimeEpisode) -> Self {
        Self {
            ep: v.ep,
            sort: v.sort,
            air_date: v.air_date,
            title: v.title.into_iter().map(Into::into).collect(),
            duration_seconds: v.duration_seconds,
            desc: v.desc,
            ex_id: v.ex_id.into(),
        }
    }
}

impl From<AnimeEpisodeItem> for AnimeEpisode {
    fn from(v: AnimeEpisodeItem) -> Self {
        Self {
            ep: v.ep,
            sort: v.sort,
            air_date: v.air_date,
            title: v.title.into_iter().map(Into::into).collect(),
            duration_seconds: v.duration_seconds,
            desc: v.desc,
            ex_id: v.ex_id.into(),
        }
    }
}

/// 季度信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeSeasonItem {
    /// 来源平台
    pub target: AnimeSourceTargetItem,
    /// 语言
    pub lang: AnimeLangTargetItem,
    /// 季度简介
    pub desc: String,
    /// 季度序号
    pub season: u32,
    /// 该季度包含的剧集列表
    pub eps: Vec<AnimeEpisodeItem>,
    /// 计划的总剧集数
    pub planned_episode_count: u32,
}

impl From<AnimeSeason> for AnimeSeasonItem {
    fn from(v: AnimeSeason) -> Self {
        Self {
            target: v.target.into(),
            lang: v.lang.into(),
            desc: v.desc,
            season: v.season,
            eps: v.eps.into_iter().map(Into::into).collect(),
            planned_episode_count: v.planned_episode_count,
        }
    }
}

impl From<AnimeSeasonItem> for AnimeSeason {
    fn from(v: AnimeSeasonItem) -> Self {
        Self {
            target: v.target.into(),
            lang: v.lang.into(),
            desc: v.desc,
            season: v.season,
            eps: v.eps.into_iter().map(Into::into).collect(),
            planned_episode_count: v.planned_episode_count,
        }
    }
}

/// 番剧元数据详情
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeMetadataItem {
    /// 外部关联信息 (如 Bangumi ID, TMDB ID 等)
    pub external_link: Vec<AnimeExItem>,
    /// 多语言标题集合
    pub titles: Vec<AnimeTitleItem>,
    /// 放送星期几
    pub air_weekday: AnimeAirWeekdayItem,
    /// 放送首播日期
    pub air_date: NaiveDate,
    /// 放送季度 (例如 2024年秋季即 20244)
    pub air_quarter: u32,
    /// 各季度具体信息
    pub season: Vec<AnimeSeasonItem>,
}

impl From<AnimeMetadata> for AnimeMetadataItem {
    fn from(v: AnimeMetadata) -> Self {
        Self {
            external_link: v.external_link.into_iter().map(Into::into).collect(),
            titles: v.titles.into_iter().map(Into::into).collect(),
            air_weekday: v.air_weekday.into(),
            air_date: v.air_date,
            air_quarter: v.air_quarter,
            season: v.season.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<AnimeMetadataItem> for AnimeMetadata {
    fn from(v: AnimeMetadataItem) -> Self {
        Self {
            external_link: v.external_link.into_iter().map(Into::into).collect(),
            titles: v.titles.into_iter().map(Into::into).collect(),
            air_weekday: v.air_weekday.into(),
            air_date: v.air_date,
            air_quarter: v.air_quarter,
            season: v.season.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QuarterStat {
    /// 番剧放送季度，例如 202401
    pub quarter: u32,
    /// 该季度入库的番剧总数
    pub total_count: i64,
    /// 当前用户在该季度订阅的番剧总数
    pub sub_count: i64,
    /// 当前用户在该季度已经标记为“完结”状态（即已达到预定集数）的番剧数
    pub completed_count: i64,
    /// 搜索状态为“不搜索”的番剧数
    pub not_search_count: i64,
    /// 搜索状态为“等待搜索”的番剧数
    pub pending_count: i64,
    /// 搜索状态为“匹配中”的番剧数
    pub matching_count: i64,
    /// 搜索状态为“正在搜索”的番剧数
    pub searching_count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BackoffFeed {
    /// 处于退避状态（被系统临时阻断）的订阅源ID
    pub feed_id: i64,
    /// 订阅源名称
    pub feed_name: String,
    /// 退避结束时间（Unix时间戳，秒级）
    pub backoff_until: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SystemStatResponse {
    /// 系统已入库的番剧总数
    pub total_anime_count: i64,
    /// 当前用户总共订阅的番剧数
    pub user_subscribed_count: i64,
    /// 系统中当前正在等待执行的搜索委托任务数量
    pub waiting_mandates_count: i64,
    /// 由于请求失败过多，当前正处于退避等待期的订阅源列表
    pub backoff_feeds: Vec<BackoffFeed>,
    /// 分季度的番剧统计与订阅进度列表
    pub quarter_stats: Vec<QuarterStat>,
}
