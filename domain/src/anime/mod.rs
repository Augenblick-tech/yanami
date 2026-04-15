use std::fmt;

use async_trait::async_trait;

use crate::shared::error::DomainError;

pub mod capability;

/// 番剧目录中的稳定业务标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimeId(pub i64);

/// 番剧放送星期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BroadcastWeekday(pub i64);

/// 当前季度预期总集数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlannedEpisodeCount(pub i64);

/// 当前季度编号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeasonNumber(pub i64);

impl fmt::Display for SeasonNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}", self.0)
    }
}

/// 番剧首播日期。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AirDate(pub String);

/// 番剧用于匹配和展示的标题集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeTitleSet {
    /// 日文原名。
    pub original_ja: String,
    /// 简中标题。
    pub localized_zh_cn: String,
    /// 繁中标题。
    pub localized_zh_tw: String,
    /// 主动搜索使用的标准检索名。
    pub search_name: String,
    /// 上游返回的其他候选别名。
    pub aliases: Vec<String>,
}

/// 上游同步得到的一条番剧元数据快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeMetadata {
    /// 番剧稳定标识。
    pub id: AnimeId,
    /// 多语言标题集合。
    pub titles: AnimeTitleSet,
    /// 放送星期。
    pub broadcast_weekday: BroadcastWeekday,
    /// 当前季度预期总集数。
    pub planned_episode_count: PlannedEpisodeCount,
    /// 首播日期。
    pub air_date: AirDate,
    /// 当前季度编号。
    pub season: SeasonNumber,
}

impl AnimeMetadata {
    /// 按优先级返回最适合做目录名的标题：search_name → zh_cn → zh_tw → original_ja。
    pub fn series_name(&self) -> String {
        let titles = &self.titles;
        [&titles.search_name, &titles.localized_zh_cn, &titles.localized_zh_tw, &titles.original_ja]
            .iter()
            .find(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| self.id.0.to_string())
    }

    /// 返回所有标题变体作为搜索关键词。
    pub fn search_keywords(&self) -> Vec<String> {
        let titles = &self.titles;
        let mut keywords = vec![
            titles.original_ja.clone(),
            titles.localized_zh_cn.clone(),
            titles.localized_zh_tw.clone(),
            titles.search_name.clone(),
        ];
        keywords.extend(titles.aliases.clone());
        keywords.retain(|k| !k.trim().is_empty());
        keywords
    }

    pub fn quarter(&self) -> Result<(i32, u32), DomainError> {
        let air_date = &self.air_date.0;
        let year = air_date.get(0..4).and_then(|v| v.parse::<i32>().ok());
        let month = air_date.get(5..7).and_then(|v| v.parse::<u32>().ok());
        let (Some(year), Some(month)) = (year, month) else {
            return Err(DomainError::InvariantViolation(
                "air date must be yyyy-mm-dd",
            ));
        };
        Ok(match month {
            12 => (year + 1, 1),
            1..=2 => (year, 1),
            3..=5 => (year, 4),
            6..=8 => (year, 7),
            9..=11 => (year, 10),
            _ => {
                return Err(DomainError::InvariantViolation("air date month is invalid"));
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceAnimeMetadataResult {
    pub new_anime_ids: Vec<AnimeId>,
}

/// 番剧列表查询过滤条件。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimeListQuery {
    /// 是否按锁定状态过滤。
    pub metadata_locked: Option<bool>,
    /// 标题关键字。
    pub keyword: Option<String>,
    /// 季度年份。
    pub year: Option<i32>,
    /// 季度月份，只允许 1、4、7、10。
    pub month: Option<u32>,
}

/// 持久化番剧元数据的仓储端口。
#[async_trait]
pub trait AnimeMetadataRepository: Send + Sync {
    /// 创建一条新的番剧元数据记录。
    async fn create_anime_metadata(&self, metadata: &AnimeMetadata) -> Result<(), DomainError>;

    /// 用当前季度完整快照替换本地元数据。
    async fn replace_anime_metadata(
        &self,
        entries: &[AnimeMetadata],
    ) -> Result<ReplaceAnimeMetadataResult, DomainError>;
}

/// Anime context 下的状态仓储端口。
#[async_trait]
pub trait AnimeStateRepository: Send + Sync {
    /// 更新元数据锁定状态。
    async fn set_metadata_locked(&self, anime_id: AnimeId, locked: bool)
        -> Result<(), DomainError>;
}
