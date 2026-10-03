use anime::entity::model::{
    AnimeAirWeekday, AnimeEpisode, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeMetadata,
    AnimeSearchResult, AnimeSeason, AnimeSeriesMetadata, AnimeSourceTarget, AnimeTitle,
};
use chrono::NaiveDate;
use feed::entity::feed_entity::FeedEntity;
use serde::{Deserialize, Serialize};
use subscription::entity::episode_entity::EpsiodeEntity;
use subscription::entity::rule_entity::RuleEntity;
use user::entity::model::{
    DefaultDownloaderConfig, DownloadConfig, DownloaderConfig, QbitConfig, UserRole,
};
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
pub struct SwitchActiveDownloaderRequest {
    pub name: String,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct DownloadTaskResponse {
    #[schema(example = "abcdef1234567890abcdef1234567890abcdef12")]
    pub hash: String,
    pub name: String,
    #[schema(example = "Downloading")]
    pub state: String,
    #[schema(example = 0.5)]
    pub progress: f64,
    /// 总大小，单位: 字节 (Bytes)
    pub total_size: u64,
    /// 下载速率，单位: 字节/秒 (B/s)
    pub download_speed: u64,
    /// 是否正在做种
    pub is_seeding: bool,
    /// 上传速率，单位: 字节/秒 (B/s)
    pub upload_speed: u64,
    /// 做种率 (已上传/总大小)
    pub seed_ratio: f64,
    /// 做种时长，单位: 秒 (s)
    pub seed_duration: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub enum DownloadTaskAction {
    #[serde(rename = "pause")]
    Pause,
    #[serde(rename = "resume")]
    Resume,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DownloadTaskActionRequest {
    #[schema(example = "pause")]
    pub action: DownloadTaskAction,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BindRuleRequest {
    /// 规则 ID
    #[schema(example = 1)]
    pub rule_id: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PageAnimeRequest {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub keyword: Option<String>,
    /// 目标语言名称过滤。
    ///
    /// 支持传入以下字符串（不区分大小写）：
    /// - 日语: `jp`, `ja`
    /// - 简体中文: `zh_cn`, `cn`, `zh-hans`
    /// - 繁体中文: `zh_tw`, `tw`, `zh-hant`
    /// - 英语: `en`
    /// - 韩语: `kr`, `ko`
    /// - 其他自定义语言代码亦可（如 `fr`）
    #[schema(example = "zh_cn")]
    pub lang: Option<String>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub subscription: Option<bool>,
    /// 搜索补全状态过滤:
    /// - 0 = 不搜索 (NotSearch)
    /// - 1 = 等待中 (Pending)
    /// - 2 = 匹配中 (Matching)
    /// - 3 = 搜索中 (Searching)
    #[schema(example = 0)]
    pub search_status: Option<i64>,
    /// 订阅状态过滤:
    /// - 1 = 已订阅 (Subscribed, 等同于 subscription = true)
    /// - 2 = 已完结 (Completed, 更新进度 >= 总集数)
    /// - 3 = 未开始 (Not Started, 进度 = 0)
    /// - 4 = 更新中 (Updating, 0 < 进度 < 总集数)
    #[schema(example = 1)]
    pub status: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AnimeResponse {
    pub id: i64,
    pub name: String,
    pub name_target: Option<String>,
    pub desc: String,
    pub air_date: String,
    /// 放送星期几 (数字1-7代表周一到周日)
    #[schema(example = 1)]
    pub air_weekday: i64,
    pub eps: u32,
    /// 季度编号 (第几季)
    #[schema(example = 1)]
    pub season: u32,
    pub sub_info: Option<AnimeSubInfo>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AnimeSubInfo {
    pub sub_anime_id: i64,
    /// 搜索状态: 0=不搜索(NotSearch), 1=等待中(Pending), 2=匹配中(Matching), 3=搜索中(Searching)
    #[schema(example = 0)]
    pub search_status: i32,
    pub progress: u32,
    pub rule_id: Option<i64>,
    pub rule_name: Option<String>,
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
pub struct RecentEpisodeResponse {
    /// 原始番名
    pub origin_name: String,
    /// 对应语言的番名
    pub lang_name: Option<String>,
    /// 剧集集数
    #[schema(example = 1.0)]
    pub ep_num: Option<f64>,
    /// 匹配规则名
    pub rule_name: Option<String>,
    /// 更新时间 (Unix 时间戳)
    pub updated_at: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecentEpisodeQuery {
    /// 目标语言名称。
    ///
    /// 支持传入以下字符串（不区分大小写）：
    /// - 日语: `jp`, `ja`
    /// - 简体中文: `zh_cn`, `cn`, `zh-hans`
    /// - 繁体中文: `zh_tw`, `tw`, `zh-hant`
    /// - 英语: `en`
    /// - 韩语: `kr`, `ko`
    /// - 其他自定义语言代码亦可（如 `fr`）
    #[schema(example = "zh_cn")]
    pub lang: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchStatusRequest {
    pub enable: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// 用户标识
    pub user_id: i64,
    /// 用户角色：1=管理员(Admin), 2=普通用户(User)
    #[schema(example = 1)]
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
pub struct DefaultDownloaderSettings {
    /// 最小做种时间 (分钟)
    pub max_seed_time: Option<u64>,
    /// 最大分享率
    pub max_seed_ratio: Option<f64>,
    /// 最大上传速度 (KB/s)
    pub max_upload_speed: Option<u64>,
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
    Default(DownloadSettings<DefaultDownloaderSettings>),
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

impl From<DefaultDownloaderConfig> for DefaultDownloaderSettings {
    fn from(config: DefaultDownloaderConfig) -> Self {
        Self {
            max_seed_time: config.max_seed_time,
            max_seed_ratio: config.max_seed_ratio,
            max_upload_speed: config.max_upload_speed,
        }
    }
}

impl From<DefaultDownloaderSettings> for DefaultDownloaderConfig {
    fn from(settings: DefaultDownloaderSettings) -> Self {
        Self {
            max_seed_time: settings.max_seed_time,
            max_seed_ratio: settings.max_seed_ratio,
            max_upload_speed: settings.max_upload_speed,
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

impl From<DownloadConfig<DefaultDownloaderConfig>> for DownloadSettings<DefaultDownloaderSettings> {
    fn from(config: DownloadConfig<DefaultDownloaderConfig>) -> Self {
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

impl From<DownloadSettings<DefaultDownloaderSettings>> for DownloadConfig<DefaultDownloaderConfig> {
    fn from(settings: DownloadSettings<DefaultDownloaderSettings>) -> Self {
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
            DownloaderConfig::Default(c) => DownloaderSettings::Default(c.into()),
        }
    }
}

impl From<DownloaderSettings> for DownloaderConfig {
    fn from(settings: DownloaderSettings) -> Self {
        match settings {
            DownloaderSettings::Qbit(c) => DownloaderConfig::Qbit(c.into()),
            DownloaderSettings::Default(c) => DownloaderConfig::Default(c.into()),
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
    /// 剧集状态: 0=未下载(Pending), 1=已完成(Downloaded)
    #[schema(example = 0)]
    pub status: i32,
    /// 剧集集数
    #[schema(example = 1.0)]
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

/// 编辑番剧请求
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct EditAnimeRequest {
    /// 番剧完整元数据
    pub metadata: AnimeMetadataItem,
    /// 是否锁定元数据 (锁定后自动任务将不再覆盖这些数据)
    pub lock: Option<bool>,
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
    /// 日语
    JP,
    /// 简体中文
    ZhCn,
    /// 繁体中文
    ZhTw,
    /// 英语
    EN,
    /// 韩语
    KR,
    /// 其他语言 (附带语言代码字符串)
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

/// 系列展示信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct AnimeSeriesMetadataItem {
    /// 系列原名
    pub origin_name: String,
    /// 系列中文名
    pub cn_name: String,
    /// 系列简介
    pub desc: String,
    /// 系列首播日期
    pub air_date: NaiveDate,
    /// 系列题材类型
    pub genres: Vec<String>,
}

impl From<AnimeSeriesMetadata> for AnimeSeriesMetadataItem {
    fn from(v: AnimeSeriesMetadata) -> Self {
        Self {
            origin_name: v.origin_name,
            cn_name: v.cn_name,
            desc: v.desc,
            air_date: v.air_date,
            genres: v.genres,
        }
    }
}

impl From<AnimeSeriesMetadataItem> for AnimeSeriesMetadata {
    fn from(v: AnimeSeriesMetadataItem) -> Self {
        Self {
            origin_name: v.origin_name,
            cn_name: v.cn_name,
            desc: v.desc,
            air_date: v.air_date,
            genres: v.genres,
        }
    }
}

/// 番剧元数据详情
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnimeMetadataItem {
    /// 系列信息：只有具备 TMDB 系列身份的番剧才有，取不到就是 None
    pub series_metadata: Option<AnimeSeriesMetadataItem>,
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
            series_metadata: v.series_metadata.map(Into::into),
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
            // 系列信息随请求体一起进来：与同步链路同构，与番剧本体一次写入
            series_metadata: v.series_metadata.map(Into::into),
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
    /// 该季度已收录的番剧总数
    pub total_count: i64,
    /// 当前用户在该季度订阅的番剧总数
    pub sub_count: i64,
    /// 订阅了但更新进度为0（未开始）的番剧数
    pub not_started_count: i64,
    /// 更新进度大于0但未完结（更新中）的番剧数
    pub updating_count: i64,
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
    /// 系统已收录的番剧总数
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

#[cfg(test)]
mod tests {
    use super::*;
    // web 未直接依赖 serde_json，这里复用 utoipa 重新导出的 serde_json（无需新增依赖）
    use utoipa::r#gen::serde_json;

    /// 构造带系列信息的元数据请求体（字段取自真实抓取的 Re:Zero 条目）。
    fn metadata_item_with_series(
        series_metadata: Option<AnimeSeriesMetadataItem>,
    ) -> AnimeMetadataItem {
        AnimeMetadataItem {
            series_metadata,
            external_link: vec![AnimeExItem {
                id: AnimeIdTypeItem::Int(65942),
                target: AnimeSourceTargetItem::TMDB,
                r#type: Some("tv".to_string()),
            }],
            titles: vec![AnimeTitleItem {
                name: "Re：从零开始的异世界生活".to_string(),
                match_name: "re从零开始的异世界生活".to_string(),
                target: AnimeLangTargetItem::ZhCn,
                origin: false,
            }],
            air_weekday: AnimeAirWeekdayItem::Saturday,
            air_date: NaiveDate::from_ymd_opt(2024, 10, 2).expect("valid air date"),
            air_quarter: 202410,
            season: vec![],
        }
    }

    fn re_zero_series_metadata() -> AnimeSeriesMetadataItem {
        AnimeSeriesMetadataItem {
            origin_name: "Re:ゼロから始める異世界生活".to_string(),
            cn_name: "Re：从零开始的异世界生活".to_string(),
            desc: "系列简介".to_string(),
            air_date: NaiveDate::from_ymd_opt(2016, 4, 4).expect("valid series air date"),
            genres: vec!["动画".to_string(), "悬疑".to_string()],
        }
    }

    #[test]
    fn test_anime_metadata_item_keeps_series_metadata_from_request_body() {
        // 请求体的系列信息必须进入领域元数据，否则 repository 不会写 anime_series
        let item = metadata_item_with_series(Some(re_zero_series_metadata()));

        let metadata: AnimeMetadata = item.into();
        let series = metadata
            .series_metadata
            .expect("series metadata from request body should be kept");
        assert_eq!(series.origin_name, "Re:ゼロから始める異世界生活");
        assert_eq!(series.cn_name, "Re：从零开始的异世界生活");
        assert_eq!(series.genres, vec!["动画".to_string(), "悬疑".to_string()]);
        assert_eq!(
            series.air_date,
            NaiveDate::from_ymd_opt(2016, 4, 4).expect("valid series air date")
        );
    }

    #[test]
    fn test_anime_metadata_item_returns_series_metadata_to_client() {
        // 反向：领域元数据转 DTO 时也要把系列信息带出去（bgm_info 响应靠它回传给前端）
        let metadata: AnimeMetadata =
            metadata_item_with_series(Some(re_zero_series_metadata())).into();

        let back: AnimeMetadataItem = metadata.into();
        assert_eq!(back.series_metadata, Some(re_zero_series_metadata()));
    }

    #[test]
    fn test_anime_metadata_item_keeps_absent_series_metadata() {
        // 没有 TMDB 系列身份的番剧取不到系列信息，不能凭请求体伪造
        let metadata: AnimeMetadata = metadata_item_with_series(None).into();
        assert!(metadata.series_metadata.is_none());
    }

    #[test]
    fn test_page_fields_are_preserved() {
        let page = Page {
            page: 2,
            page_size: 20,
            total: 42,
            data: vec![1_u32, 2, 3],
        };

        assert_eq!(page.page, 2);
        assert_eq!(page.page_size, 20);
        assert_eq!(page.total, 42);
        assert_eq!(page.data, vec![1_u32, 2, 3]);
    }

    #[test]
    fn test_page_deserializes_generic_data() {
        let raw = r#"{"page":1,"page_size":10,"total":3,"data":[1,2,3]}"#;
        let page: Page<Vec<u32>> = serde_json::from_str(raw).expect("page should deserialize");

        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 10);
        assert_eq!(page.total, 3);
        assert_eq!(page.data, vec![1_u32, 2, 3]);
    }

    #[test]
    fn test_page_anime_request_full_deserialisation() {
        // 与前端实际提交的 JSON 格式保持一致
        let raw = r#"{
            "page": 1,
            "page_size": 20,
            "keyword": "败犬女主",
            "lang": "zh_cn",
            "year": 2024,
            "month": 10,
            "subscription": true,
            "search_status": 2,
            "status": 4
        }"#;

        let request: PageAnimeRequest =
            serde_json::from_str(raw).expect("full request json should deserialize");

        assert_eq!(request.page, Some(1));
        assert_eq!(request.page_size, Some(20));
        assert_eq!(request.keyword.as_deref(), Some("败犬女主"));
        assert_eq!(request.lang.as_deref(), Some("zh_cn"));
        assert_eq!(request.year, Some(2024));
        assert_eq!(request.month, Some(10));
        assert_eq!(request.subscription, Some(true));
        assert_eq!(request.search_status, Some(2));
        assert_eq!(request.status, Some(4));
    }

    #[test]
    fn test_page_anime_request_missing_fields_are_none() {
        let request: PageAnimeRequest =
            serde_json::from_str("{}").expect("empty json should deserialize to all none");

        assert!(request.page.is_none());
        assert!(request.page_size.is_none());
        assert!(request.keyword.is_none());
        assert!(request.lang.is_none());
        assert!(request.year.is_none());
        assert!(request.month.is_none());
        assert!(request.subscription.is_none());
        assert!(request.search_status.is_none());
        assert!(request.status.is_none());
    }

    #[test]
    fn test_page_anime_request_rejects_wrong_type() {
        // page 应为无符号整数，传字符串必须反序列化失败
        let result: Result<PageAnimeRequest, _> = serde_json::from_str(r#"{"page":"第一页"}"#);
        assert!(result.is_err());

        // month 为 u32，负数必须反序列化失败
        let result: Result<PageAnimeRequest, _> = serde_json::from_str(r#"{"month":-1}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_api_response_ok_sets_business_code() {
        let response = ApiResponse::ok(vec![1_u32]);
        assert_eq!(response.code, 200);
        assert_eq!(response.data, vec![1_u32]);
    }
}
