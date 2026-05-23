use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use domain::{
    anime::AnimeTitleSet,
    feed::FeedSource,
    rule::MatchingRule,
    user::{User, UserRole},
};
use service::{
    anime::service::{
        AnimeDashboardStats, AnimeDashboardView, AnimeItemView, AnimeReleaseRecordView,
        LatestAnimeView,
    },
    user::service::LoginOutcome,
};

// ── Request ──────────────────────────────────────────────────────────────────

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
    pub old_password: String,
    /// 新密码
    pub new_password: String,
}

/// 创建番剧的请求体（预览和提交共用）。
///
/// - `GET /api/v1/animes/preview?bgm_id=123` 返回此结构，前端展示。
/// - `POST /api/v1/animes` 接收此结构，用户可修改预览结果后提交。
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateAnimeRequest {
    /// Bangumi 番剧 ID（subject_id）。
    #[schema(example = 123456)]
    pub bgm_id: i64,

    /// 日文原名。
    #[schema(example = "葬送のフリーレン")]
    pub original_ja: String,

    /// 简中标题。
    #[schema(example = "葬送的芙莉莲")]
    pub localized_zh_cn: String,

    /// 繁中标题。
    #[schema(example = "葬送的芙蓮")]
    pub localized_zh_tw: String,

    /// 主动搜索使用的标准检索名。
    #[schema(example = "Frieren")]
    pub search_name: String,

    /// 候选别名。
    #[schema(example = json!(["Sousou no Frieren"]))]
    pub aliases: Vec<String>,

    /// 放送星期。0=日 1=一 2=二 3=三 4=四 5=五 6=六。
    #[schema(value_type = i64, minimum = 0, maximum = 6, example = 5)]
    pub broadcast_weekday: i64,

    /// 当前季度预期总集数。
    #[schema(value_type = i64, minimum = 1, example = 12)]
    pub planned_episode_count: i64,

    /// 首播日期。格式 yyyy-mm-dd。
    #[schema(example = "2026-04-01", pattern = "^\\d{4}-\\d{2}-\\d{2}$")]
    pub air_date: String,

    /// 当前季度编号。
    #[schema(value_type = i64, minimum = 1, example = 1)]
    pub season: i64,
}

/// 更新番剧元数据的请求体（同时也是编辑预填响应体）。
///
/// - `GET /api/v1/animes/{anime_id}/metadata` 返回此结构，前端用于填充编辑表单。
/// - `PUT /api/v1/animes/{anime_id}/metadata` 接收此结构，提交修改后的元数据。
///
/// 番剧 ID（bgm_id）不可修改，通过 URL 路径参数传递。
/// 所有字段均为必填。
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateAnimeMetadataRequest {
    /// 日文原名。不可为空。
    #[schema(example = "葬送のフリーレン")]
    pub original_ja: String,

    /// 简中标题。
    #[schema(example = "葬送的芙莉莲")]
    pub localized_zh_cn: String,

    /// 繁中标题。
    #[schema(example = "葬送的芙蓮")]
    pub localized_zh_tw: String,

    /// 主动搜索使用的标准检索名。
    #[schema(example = "Frieren")]
    pub search_name: String,

    /// 上游返回的其他候选别名。
    #[schema(example = json!(["Sousou no Frieren"]))]
    pub aliases: Vec<String>,

    /// 放送星期。0=日 1=一 2=二 3=三 4=四 5=五 6=六。
    #[schema(value_type = i64, minimum = 0, maximum = 6, example = 5)]
    pub broadcast_weekday: i64,

    /// 当前季度预期总集数。必须大于 0。
    #[schema(value_type = i64, minimum = 1, example = 12)]
    pub planned_episode_count: i64,

    /// 首播日期。格式 yyyy-mm-dd。
    #[schema(example = "2026-04-01", pattern = "^\\d{4}-\\d{2}-\\d{2}$")]
    pub air_date: String,

    /// 当前季度编号。必须大于 0。
    #[schema(value_type = i64, minimum = 1, example = 1)]
    pub season: i64,
}

/// 设置番剧订阅活跃状态。
///
/// - `enabled: true` → 启用追更
/// - `enabled: false` → 暂停追更
///
/// 仅用于 `PUT /animes/{anime_id}/subscription/active`。
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetSubscriptionRequest {
    /// 是否启用追更。
    ///
    /// - `true` — 启用
    /// - `false` — 暂停
    #[schema(example = true)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchAnimeRequest {
    /// 是否启用主动搜索补全
    pub search_enabled: Option<bool>,
    /// 是否锁定元数据，锁定后上游更新不再覆盖
    pub metadata_locked: Option<bool>,
}

/// 设置自动订阅请求体。
///
/// 开启后，系统定时同步番剧日历发现新番时，会为当前个人空间自动创建启用状态的订阅。
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetAutoSubscribeRequest {
    /// 是否开启新番自动订阅
    ///
    /// - `true` — 开启
    /// - `false` — 关闭
    #[schema(example = true)]
    pub enabled: bool,
}

/// 自动订阅状态响应。
#[derive(Debug, Serialize, ToSchema)]
pub struct AutoSubscribeResponse {
    /// 当前自动订阅状态
    ///
    /// - `true` — 已开启
    /// - `false` — 已关闭
    #[schema(example = true)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveFeedSourceRequest {
    /// 来源标识，新建时留空
    pub id: Option<String>,
    /// 来源展示名称
    #[schema(example = "动漫花园")]
    pub title: String,
    /// 站点首页 URL
    #[schema(example = "https://share.dmhy.org")]
    pub site_url: Option<String>,
    /// 搜索 URL，包含 {} 占位符
    pub search_url: Option<String>,
}

impl SaveFeedSourceRequest {
    pub fn into_domain(self, fallback_id: String) -> FeedSource {
        FeedSource {
            id: domain::feed::FeedSourceId(self.id.unwrap_or(fallback_id)),
            title: self.title,
            site_url: self.site_url,
            search_url: self.search_url,
            source_key: None,
        }
    }

    pub fn into_domain_with_id(self, id: String) -> FeedSource {
        FeedSource {
            id: domain::feed::FeedSourceId(id),
            title: self.title,
            site_url: self.site_url,
            search_url: self.search_url,
            source_key: None,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MatchingRuleRequest {
    /// 规则名称
    #[schema(example = "ANi")]
    pub name: String,
    /// 规则优先级，越小越优先
    pub order: u32,
    /// 正则表达式，用于匹配资源标题
    #[schema(example = "^\\\\[ANi\\\\].*")]
    pub pattern: String,
}

impl MatchingRuleRequest {
    pub fn into_domain(self, id: String) -> MatchingRule {
        MatchingRule {
            id: domain::rule::MatchingRuleId(id),
            name: self.name,
            order: self.order,
            pattern: self.pattern,
            active: true,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveQbitProfileRequest {
    /// qBittorrent Web UI 地址
    #[schema(example = "http://192.168.1.100:8080")]
    pub endpoint: String,
    /// qBittorrent 登录用户名
    pub username: String,
    /// qBittorrent 登录密码
    pub password: String,
    /// 下载保存根路径
    pub download_path: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SelectDownloadDriverRequest {
    /// 下载器标识。可用值由系统运行时决定，通常包括 "qbit"；开启 noop_enabled 后还包括 "noop"。
    #[schema(example = "qbit")]
    pub driver_key: String,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct AnimeIdParam {
    /// 番剧 ID
    pub anime_id: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct AnimeLanguageQuery {
    /// 展示语言。支持 ja / zh-CN / zh-TW；未命中时尝试 Accept-Language，仍未命中时回退日文原名。
    pub language: Option<String>,
}

/// Bangumi ID 查询参数，用于预览接口。
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct BgmIdQuery {
    /// Bangumi 番剧 ID
    #[param(example = 123456)]
    pub bgm_id: i64,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct LatestAnimeParam {
    /// 返回条数上限，0-20，默认 10
    #[param(minimum = 0, maximum = 20, default = 10)]
    pub limit: Option<usize>,
    /// 展示语言。支持 ja / zh-CN / zh-TW；未命中时尝试 Accept-Language，仍未命中时回退日文原名。
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct AnimeQuery {
    /// 按启用状态过滤
    pub enabled: Option<bool>,
    /// 按主动搜索状态过滤
    pub search_enabled: Option<bool>,
    /// 按订阅状态过滤：true=仅返回已订阅的番剧，false=仅返回未订阅的番剧
    pub subscribed: Option<bool>,
    /// 按元数据锁定状态过滤
    pub metadata_locked: Option<bool>,
    /// 按进度状态过滤：0=未开始 1=更新中 2=完结
    pub progress_state: Option<u8>,
    /// 搜索关键字
    pub keyword: Option<String>,
    /// 放送年份
    pub year: Option<i32>,
    /// 放送月份
    pub month: Option<u32>,
    /// 页码，从 1 开始，默认 1
    #[param(minimum = 1, default = 1)]
    pub page: Option<u32>,
    /// 每页条数，默认 20
    #[param(minimum = 1, maximum = 100, default = 20)]
    pub page_size: Option<u32>,
    /// 展示语言。支持 ja / zh-CN / zh-TW；未命中时尝试 Accept-Language，仍未命中时回退日文原名。
    pub language: Option<String>,
}

// ── Response ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    /// 业务码，0 表示成功
    pub code: i64,
    /// 响应数据
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 0, data }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// 用户标识
    pub user_id: i64,
    /// 用户角色：admin / user
    pub role: String,
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
            user_id: outcome.user_id.0,
            role: match outcome.role {
                UserRole::Admin => "admin",
                UserRole::User => "user",
            }
            .to_string(),
            access_token: outcome.access_token.access_token,
            token_type: outcome.access_token.token_type,
            expires_at: outcome.access_token.expires_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserView {
    /// 用户标识
    pub user_id: i64,
    /// 用户名
    pub username: String,
    /// 用户角色
    pub role: String,
}

impl From<User> for UserView {
    fn from(user: User) -> Self {
        Self {
            user_id: user.id.0,
            username: user.username.0,
            role: match user.role {
                UserRole::Admin => "admin",
                UserRole::User => "user",
            }
            .to_string(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChangePasswordResponse {
    /// 用户标识
    pub user_id: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FeedsResponse {
    pub sources: Vec<FeedSourceView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FeedSourceView {
    /// 来源标识
    pub id: String,
    /// 来源名称
    pub title: String,
    /// 站点首页 URL
    pub site_url: Option<String>,
    /// 搜索 URL
    pub search_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteFeedSourceResponse {
    /// 已删除的来源标识
    pub id: String,
}

impl From<FeedSource> for FeedSourceView {
    fn from(source: FeedSource) -> Self {
        Self {
            id: source.id.0,
            title: source.title,
            site_url: source.site_url,
            search_url: source.search_url,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RulesResponse {
    /// 空间标识
    pub owner_id: i64,
    pub rules: Vec<MatchingRuleView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MatchingRuleView {
    /// 规则标识
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 规则优先级
    pub order: u32,
    /// 正则表达式
    pub pattern: String,
}

impl From<MatchingRule> for MatchingRuleView {
    fn from(rule: MatchingRule) -> Self {
        Self {
            id: rule.id.0,
            name: rule.name,
            order: rule.order,
            pattern: rule.pattern,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteMatchingRuleResponse {
    /// 已失活的规则标识
    pub id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedAnimeResponse {
    /// 当前页番剧列表
    pub items: Vec<AnimeViewResponse>,
    /// 总条数
    pub total: usize,
    /// 当前页码
    pub page: u32,
    /// 每页条数
    pub page_size: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeDashboardResponse {
    /// 全部番剧统计
    pub overall: AnimeDashboardStatsResponse,
    /// 按季度划分的番剧统计，按最新季度在前排序
    pub quarters: Vec<AnimeDashboardQuarterResponse>,
    /// 搜索池统计
    pub search: AnimeDashboardSearchResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeDashboardSearchResponse {
    /// 正在搜索池中的番剧数（有未完成搜索链接的番剧）
    pub searching_anime_count: i64,
    /// 剩余待处理的搜索链接数
    pub pending_link_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeDashboardQuarterResponse {
    /// 季度年份
    pub year: i32,
    /// 季度起始月份：1 / 4 / 7 / 10
    pub month: u32,
    /// 季度标签，例如 2026-Q2
    pub label: String,
    /// 该季度统计
    pub stats: AnimeDashboardStatsResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeDashboardStatsResponse {
    /// 系统番剧总数
    pub total: usize,
    /// 已完结番剧数
    pub completed: usize,
    /// 更新中番剧数
    pub updating: usize,
    /// 未开始番剧数
    pub not_started: usize,
    /// 已订阅但暂停更新的番剧数
    pub paused: usize,
    /// 已订阅番剧数
    pub subscribed: usize,
}

impl From<AnimeDashboardStats> for AnimeDashboardStatsResponse {
    fn from(stats: AnimeDashboardStats) -> Self {
        Self {
            total: stats.total,
            completed: stats.completed,
            updating: stats.updating,
            not_started: stats.not_started,
            paused: stats.paused,
            subscribed: stats.subscribed,
        }
    }
}

impl AnimeDashboardResponse {
    pub fn from_view(view: AnimeDashboardView, search: AnimeDashboardSearchResponse) -> Self {
        Self {
            overall: view.overall.into(),
            quarters: view
                .quarters
                .into_iter()
                .map(|quarter| AnimeDashboardQuarterResponse {
                    year: quarter.year,
                    month: quarter.month,
                    label: quarter.label,
                    stats: quarter.stats.into(),
                })
                .collect(),
            search,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeViewResponse {
    /// 番剧 ID
    pub id: i64,
    /// 番剧标题
    pub title: String,
    /// 搜索状态
    pub search_state: String,
    /// 元数据是否锁定
    pub metadata_locked: bool,
    /// 是否已订阅
    pub subscribed: bool,
    /// 是否启用追更
    pub enabled: bool,
    /// 当前已匹配集数
    pub progress: u32,
    /// 命中的规则名称
    pub rule_name: Option<String>,
    /// 首播日期
    pub air_date: String,
    /// 放送星期（0=日 1=一 2=二 3=三 4=四 5=五 6=六）
    pub weekday: i64,
    /// 预期总集数
    pub planned_episodes: i64,
    /// 当前季度编号
    pub season: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LatestAnimeViewResponse {
    /// 番剧 ID
    pub anime_id: i64,
    /// 番剧标题
    pub title: String,
    /// 最新剧集号
    pub episode: u32,
    /// 匹配的规则名称
    pub rule_name: String,
    /// 更新时间戳（秒）
    pub updated_at: i64,
    /// 季度编号（第几季）
    pub season: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeReleaseRecordsResponse {
    /// 番剧 ID
    pub anime_id: i64,
    /// 更新记录
    pub records: Vec<AnimeReleaseRecordResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnimeReleaseRecordResponse {
    /// 资源 ID
    pub resource_id: String,
    /// 资源标题
    pub title: String,
    /// 资源详情或下载链接
    pub source_url: String,
    /// 匹配的规则名称
    pub rule_name: String,
    /// 资源发布时间戳（秒）
    pub published_at: Option<i64>,
    /// 记录创建时间戳（秒）
    pub created_at: i64,
}

impl From<AnimeReleaseRecordView> for AnimeReleaseRecordResponse {
    fn from(record: AnimeReleaseRecordView) -> Self {
        Self {
            resource_id: record.resource_id,
            title: record.title,
            source_url: record.source_url,
            rule_name: record.rule_name,
            published_at: record.published_at,
            created_at: record.created_at,
        }
    }
}

impl LatestAnimeViewResponse {
    pub fn from_view(view: LatestAnimeView, language: DisplayLanguagePreference<'_>) -> Self {
        Self {
            anime_id: view.metadata.id.0,
            title: select_display_title(&view.metadata.titles, language),
            episode: view.episode,
            rule_name: view.rule_name,
            updated_at: view.updated_at,
            season: view.metadata.season.0,
        }
    }
}

impl AnimeViewResponse {
    pub fn from_item(item: AnimeItemView, language: DisplayLanguagePreference<'_>) -> Self {
        Self {
            id: item.anime.id.0,
            title: select_display_title(&item.anime.metadata.titles, language),
            search_state: match item.anime.search_state {
                domain::subscription::SubscriptionSearchState::Stopped => "stopped",
                domain::subscription::SubscriptionSearchState::Pending => "pending",
                domain::subscription::SubscriptionSearchState::Running => "running",
                domain::subscription::SubscriptionSearchState::LocalMatch => "local_match",
            }
            .to_string(),
            metadata_locked: item.anime.metadata_locked,
            subscribed: item.subscription.subscribed,
            enabled: item.subscription.enabled,
            progress: item.subscription.progress,
            rule_name: item.subscription.matched_rule_name,
            air_date: item.anime.metadata.air_date.0.clone(),
            weekday: item.anime.metadata.broadcast_weekday.0,
            planned_episodes: item.anime.metadata.planned_episode_count.0,
            season: item.anime.metadata.season.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayLanguagePreference<'a> {
    pub query_language: Option<&'a str>,
    pub accept_language: Option<&'a str>,
}

fn select_display_title(titles: &AnimeTitleSet, language: DisplayLanguagePreference<'_>) -> String {
    select_title_by_language(titles, language.query_language)
        .or_else(|| select_title_from_accept_language(titles, language.accept_language))
        .unwrap_or(&titles.original_ja)
        .clone()
}

fn select_title_from_accept_language<'a>(
    titles: &'a AnimeTitleSet,
    accept_language: Option<&str>,
) -> Option<&'a String> {
    accept_language?
        .split(',')
        .filter_map(|value| value.split(';').next())
        .find_map(|language| select_title_by_language(titles, Some(language)))
}

fn select_title_by_language<'a>(
    titles: &'a AnimeTitleSet,
    language: Option<&str>,
) -> Option<&'a String> {
    let requested = language
        .map(|value| value.trim().to_ascii_lowercase().replace('_', "-"))
        .unwrap_or_default();
    let candidate = match requested.as_str() {
        "ja" | "ja-jp" | "jp" => Some(&titles.original_ja),
        "zh" | "zh-cn" | "zh-hans" | "zh-sg" | "cn" => Some(&titles.localized_zh_cn),
        "zh-tw" | "zh-hant" | "zh-hk" | "zh-mo" | "tw" | "hk" => Some(&titles.localized_zh_tw),
        _ => None,
    };
    candidate.filter(|title| !title.trim().is_empty())
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UsizeApiResponse {
    pub count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles() -> AnimeTitleSet {
        AnimeTitleSet {
            original_ja: "葬送のフリーレン".to_string(),
            localized_zh_cn: "葬送的芙莉莲".to_string(),
            localized_zh_tw: "葬送的芙蓮".to_string(),
            search_name: "frieren".to_string(),
            aliases: vec![],
        }
    }

    #[test]
    fn select_display_title_uses_requested_language_and_falls_back_to_japanese() {
        let mut titles = titles();

        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("zh-CN"),
                    accept_language: None,
                },
            ),
            "葬送的芙莉莲"
        );
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("zh_TW"),
                    accept_language: None,
                },
            ),
            "葬送的芙蓮"
        );
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("ja"),
                    accept_language: None,
                },
            ),
            "葬送のフリーレン"
        );
        assert_eq!(
            select_display_title(&titles, DisplayLanguagePreference::default()),
            "葬送のフリーレン"
        );

        titles.localized_zh_cn.clear();
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("zh-CN"),
                    accept_language: None,
                },
            ),
            "葬送のフリーレン"
        );
    }

    #[test]
    fn select_display_title_uses_accept_language_when_query_language_is_missing() {
        let titles = titles();

        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: None,
                    accept_language: Some("zh-TW,zh;q=0.9,ja;q=0.8"),
                },
            ),
            "葬送的芙蓮"
        );
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("zh-CN"),
                    accept_language: Some("zh-TW,zh;q=0.9"),
                },
            ),
            "葬送的芙莉莲"
        );
    }

    #[test]
    fn select_display_title_falls_through_query_then_header_then_japanese() {
        let mut titles = titles();

        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("en"),
                    accept_language: Some("zh-CN,zh;q=0.9"),
                },
            ),
            "葬送的芙莉莲"
        );
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("zh-TW"),
                    accept_language: Some("zh-CN,zh;q=0.9"),
                },
            ),
            "葬送的芙蓮"
        );
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("ja"),
                    accept_language: Some("zh-CN,zh;q=0.9"),
                },
            ),
            "葬送のフリーレン"
        );

        titles.localized_zh_cn.clear();
        titles.localized_zh_tw.clear();
        assert_eq!(
            select_display_title(
                &titles,
                DisplayLanguagePreference {
                    query_language: Some("en"),
                    accept_language: Some("zh-CN,zh;q=0.9"),
                },
            ),
            "葬送のフリーレン"
        );
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DownloadConfigurationResponse {
    /// 当前下载器标识。未选择时为空。
    pub driver_key: Option<String>,
    /// qBittorrent 配置摘要。未配置时为空。
    pub qbit_profile: Option<QbitProfileResponse>,
    /// 当前系统可用的下载驱动标识列表。
    pub available_drivers: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QbitProfileResponse {
    /// qBittorrent 地址
    pub endpoint: String,
    /// 登录用户名
    pub username: String,
    /// 下载保存路径
    pub download_path: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DriverResponse {
    /// 当前下载器标识
    pub driver_key: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PingResponse {
    pub status: String,
}
