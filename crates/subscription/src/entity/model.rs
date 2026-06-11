use chrono::NaiveDate;
use feed::entity::model::FeedItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SubAnimeListQuery {
    pub anime_id: Option<i64>,
    pub space_id: Option<i64>,

    pub search_status: Option<SubAnimeSearchStatus>,
    pub sub_status: Option<SubAnimeStatus>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SubAnimeSearchStatus {
    NotSearch,
    Pending,
    Matching,
    Searching,
}

impl From<SubAnimeSearchStatus> for i32 {
    fn from(status: SubAnimeSearchStatus) -> Self {
        match status {
            SubAnimeSearchStatus::NotSearch => 0,
            SubAnimeSearchStatus::Pending => 1,
            SubAnimeSearchStatus::Matching => 2,
            SubAnimeSearchStatus::Searching => 3,
        }
    }
}

impl TryFrom<i32> for SubAnimeSearchStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SubAnimeSearchStatus::NotSearch),
            1 => Ok(SubAnimeSearchStatus::Pending),
            2 => Ok(SubAnimeSearchStatus::Matching),
            3 => Ok(SubAnimeSearchStatus::Searching),
            _ => Err(format!("unknown sub anime search status type: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubAnimeStatus {
    Enable,
    Completed,
}

impl From<SubAnimeStatus> for u8 {
    fn from(value: SubAnimeStatus) -> Self {
        match value {
            SubAnimeStatus::Enable => 1,
            SubAnimeStatus::Completed => 2,
        }
    }
}

impl TryFrom<u8> for SubAnimeStatus {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SubAnimeStatus::Enable),
            2 => Ok(SubAnimeStatus::Completed),
            _ => Err(format!("unknown status value {}", value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubAnimeBaseData {
    pub id: i64,
    pub anime_id: i64,
    pub space_id: i64,
    pub rule_id: Option<i64>,

    pub search_status: SubAnimeSearchStatus,
    pub progress: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubAnimeExtendData {
    pub eps: u32,
    pub rule_name: Option<String>,
    pub titles: Vec<String>,
    pub air_date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubAnimeProps {
    pub data: SubAnimeBaseData,
    pub extend: SubAnimeExtendData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 所属订阅空间
    pub space_id: i64,
    /// 规则展示名。
    pub name: String,
    /// 规则顺序，值越小优先级越高。
    pub order: i64,
    /// 可用于匹配资源标题的表达式。
    pub pattern: String,
}

#[derive(Debug, Clone)]
pub struct RuleQuery {
    pub space_id: Option<i64>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleBaseData {
    /// 规则标识。
    pub id: i64,
    /// 是否可被新订阅匹配选择。
    pub active: bool,
    pub metadata: Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleProp {
    data: RuleBaseData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpsiodeStatus {
    Pending,
    Downloaded,
}

impl From<EpsiodeStatus> for i32 {
    fn from(status: EpsiodeStatus) -> Self {
        match status {
            EpsiodeStatus::Pending => 0,
            EpsiodeStatus::Downloaded => 1,
        }
    }
}

impl TryFrom<i32> for EpsiodeStatus {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EpsiodeStatus::Pending),
            1 => Ok(EpsiodeStatus::Downloaded),
            _ => Err(format!("unknown sub anime episode status type: {}", value)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub sub_anime_id: i64,
    pub resource_id: [u8; 20],
    pub status: EpsiodeStatus,
    pub ep_num: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct EpisodeBaseData {
    pub id: i64,
    pub ep: Episode,
}

#[derive(Debug, Clone)]
pub struct EpisodeExtendData {
    pub title: String,
    pub url: String,
    pub season: u32,
    pub anime_origin_title: String,
    pub space_id: i64,
}

#[derive(Debug, Clone)]
pub struct EpisodeProp {
    pub data: EpisodeBaseData,
    pub extend: EpisodeExtendData,
}

#[derive(Debug, Clone)]
pub struct MatchedEpisode {
    pub sub_anime_id: i64,
    pub resource_id: [u8; 20],
    pub status: EpsiodeStatus,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct SearchMandateProp {
    pub data: SearchMandateBaseData,
}

#[derive(Debug, Clone)]
pub struct SearchMandateBaseData {
    pub id: i64,
    pub mandata: Mandate,
}

#[derive(Debug, Clone)]
pub struct Mandate {
    pub anime_id: i64,
    pub feed_id: i64,
    pub url: String,
}

#[derive(Debug, Clone)]
pub enum SearchManadateResult {
    Success(Vec<FeedItem>),
    Retryable(i64),
    Failure(i64),
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub page: usize,
    pub page_size: usize,
    pub total: u64,
    pub data: T,
}

#[derive(Debug, Clone)]
pub enum ClaimResult {
    Matched,
    AlreayMartched,
    Completed,
}
