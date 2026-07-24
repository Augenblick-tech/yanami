use chrono::NaiveDate;
use common::shared::error::Error;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// 番剧列表查询过滤条件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum AnimeAirWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// 数据库存储格式：Bangumi v0 标准，1=Monday ... 7=Sunday。
impl From<AnimeAirWeekday> for i64 {
    fn from(d: AnimeAirWeekday) -> i64 {
        match d {
            AnimeAirWeekday::Monday => 1,
            AnimeAirWeekday::Tuesday => 2,
            AnimeAirWeekday::Wednesday => 3,
            AnimeAirWeekday::Thursday => 4,
            AnimeAirWeekday::Friday => 5,
            AnimeAirWeekday::Saturday => 6,
            AnimeAirWeekday::Sunday => 7,
        }
    }
}

impl TryFrom<i64> for AnimeAirWeekday {
    type Error = Error;

    fn try_from(v: i64) -> Result<Self, Error> {
        match v {
            1 => Ok(AnimeAirWeekday::Monday),
            2 => Ok(AnimeAirWeekday::Tuesday),
            3 => Ok(AnimeAirWeekday::Wednesday),
            4 => Ok(AnimeAirWeekday::Thursday),
            5 => Ok(AnimeAirWeekday::Friday),
            6 => Ok(AnimeAirWeekday::Saturday),
            7 => Ok(AnimeAirWeekday::Sunday),
            _ => Err(Error::invariant("broadcast weekday out of range")),
        }
    }
}

impl TryFrom<&str> for AnimeAirWeekday {
    type Error = Error;

    fn try_from(v: &str) -> Result<Self, Error> {
        match v {
            "礼拜一" | "星期一" | "周一" => Ok(AnimeAirWeekday::Monday),
            "礼拜二" | "星期二" | "周二" => Ok(AnimeAirWeekday::Tuesday),
            "礼拜三" | "星期三" | "周三" => Ok(AnimeAirWeekday::Wednesday),
            "礼拜四" | "星期四" | "周四" => Ok(AnimeAirWeekday::Thursday),
            "礼拜五" | "星期五" | "周五" => Ok(AnimeAirWeekday::Friday),
            "礼拜六" | "星期六" | "周六" => Ok(AnimeAirWeekday::Saturday),
            "礼拜天" | "星期天" | "星期日" | "周天" => Ok(AnimeAirWeekday::Sunday),
            _ => Err(Error::invariant("broadcast weekday out of range")),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum AnimeLangTarget {
    JP,
    ZhCn,
    ZhTw,
    EN,
    KR,
    Other(String),
}

/// 数据库存储格式：Bangumi v0 标准，1=Monday ... 7=Sunday。
impl From<AnimeLangTarget> for String {
    fn from(d: AnimeLangTarget) -> String {
        match d {
            AnimeLangTarget::JP => "jp".to_string(),
            AnimeLangTarget::ZhCn => "zh_cn".to_string(),
            AnimeLangTarget::ZhTw => "zh_tw".to_string(),
            AnimeLangTarget::EN => "en".to_string(),
            AnimeLangTarget::KR => "kr".to_string(),
            AnimeLangTarget::Other(s) => s,
        }
    }
}

impl From<&str> for AnimeLangTarget {
    fn from(v: &str) -> Self {
        let s = v.to_ascii_lowercase();
        let v = s.as_str();
        match v {
            "jp" | "ja" => AnimeLangTarget::JP,
            "zh_cn" | "cn" | "zh-hans" => AnimeLangTarget::ZhCn,
            "zh_tw" | "tw" | "zh-hant" => AnimeLangTarget::ZhTw,
            "en" | "us" | "us_en" => AnimeLangTarget::EN,
            "kr" | "ko" => AnimeLangTarget::KR,
            _ => AnimeLangTarget::Other(s),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct AnimeTitle {
    pub name: String,
    pub match_name: String,
    pub target: AnimeLangTarget,
    pub origin: bool,
}

impl AnimeTitle {
    pub fn keywords(&self) -> Vec<String> {
        Self::to_keywords(&self.name)
    }

    /// 将番剧别名处理为仅包含CJK字符的搜索关键字
    pub fn to_keywords(input: &str) -> Vec<String> {
        let mut tokens = Vec::with_capacity(input.len());
        let mut current_word = String::new();

        // 1. nfkc() 迭代器会自动处理 Unicode 标准化
        // 这会将全角的 "ＡＢＣ" 转为 "ABC"，全角数字 "１" 转为 "1"
        for c in input.nfkc() {
            if c.is_alphanumeric() {
                if Self::is_cjk(c) {
                    // 遇到中日韩字符，先将前面累积的英文/数字 word 刷入 token
                    if !current_word.is_empty() {
                        tokens.push(current_word.clone());
                        current_word.clear();
                    }
                    // 中日韩字符独立作为一个 token
                    tokens.push(c.to_string());
                } else {
                    // 非中日韩字符（英文、数字、俄文、法文等），全部转小写并累积成单词
                    for lower_c in c.to_lowercase() {
                        current_word.push(lower_c);
                    }
                }
            } else {
                // is_alphanumeric 为 false 代表遇到了标点符号、空格或特殊符号
                // 此时作为单词的天然分割点，将累积的 word 刷入，并丢弃该符号
                if !current_word.is_empty() {
                    tokens.push(current_word.clone());
                    current_word.clear();
                }
            }
        }

        // 处理字符串末尾可能遗留的英文/数字单词
        if !current_word.is_empty() {
            tokens.push(current_word);
        }
        tokens
    }

    /// 基于 Unicode 码位判断是否为中日韩字符 (汉字、假名、谚文、注音等)
    fn is_cjk(c: char) -> bool {
        let u = c as u32;
        matches!(
            u,
            // CJK 统一表意文字及扩展 (涵盖简繁体及绝大部分生僻字)
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF |
            // 日文平假名、片假名、片假名语音扩展
            0x3040..=0x309F | 0x30A0..=0x30FF | 0x31F0..=0x31FF |
            // 韩文谚文音节及字母
            0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F |
            // 台湾注音符号
            0x3100..=0x312F
        )
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum AnimeIdType {
    Int(i64),
    String(String),
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum AnimeSourceTarget {
    TMDB,
    Bangumi,
    Other(String),
}

impl From<AnimeSourceTarget> for String {
    fn from(d: AnimeSourceTarget) -> String {
        match d {
            AnimeSourceTarget::Bangumi => "Bangumi".to_string(),
            AnimeSourceTarget::TMDB => "TMDB".to_string(),
            AnimeSourceTarget::Other(v) => v,
        }
    }
}

impl From<&str> for AnimeSourceTarget {
    fn from(v: &str) -> Self {
        match v {
            "Bangumi" => AnimeSourceTarget::Bangumi,
            "TMDB" => AnimeSourceTarget::TMDB,
            _ => AnimeSourceTarget::Other(v.to_string()),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct AnimeEx {
    pub id: AnimeIdType,
    pub target: AnimeSourceTarget,
    pub r#type: Option<String>,
}

/// 上游同步得到的一条番剧元数据快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeMetadata {
    /// 番剧外部链接
    pub external_link: Vec<AnimeEx>,
    /// 多语言标题集合。
    pub titles: Vec<AnimeTitle>,
    /// 放送星期。
    pub air_weekday: AnimeAirWeekday,
    /// 首播日期。
    pub air_date: NaiveDate,
    /// 放送季度 202607
    pub air_quarter: u32,
    /// 季度信息
    pub season: Vec<AnimeSeason>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeDesc {
    pub target: AnimeSourceTarget,
    pub desc: String,
    pub lang: AnimeLangTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeSeason {
    pub target: AnimeSourceTarget,
    pub lang: AnimeLangTarget,
    pub desc: String,
    /// 季度编号。
    pub season: u32,
    pub eps: Vec<AnimeEpisode>,
    /// 季度预期总集数。
    pub planned_episode_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeEpisodes {
    // 外部来源标识
    pub target: AnimeSourceTarget,
    // 剧集信息
    pub eps: Vec<AnimeEpisode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeEpisode {
    // 季度剧集绝对排序（从1开始）
    pub ep: u32,
    // 剧集编号
    pub sort: f64,
    // 发布日期
    pub air_date: NaiveDate,
    // 标题
    pub title: Vec<AnimeTitle>,
    // 时长
    pub duration_seconds: u64,
    // 梗概
    pub desc: String,
    // 外部剧集id
    pub ex_id: AnimeIdType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeBaseData {
    pub id: i64,
    pub metadata: AnimeMetadata,
    pub lock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeProps {
    pub data: AnimeBaseData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimeSearchResult {
    pub name: String,
    pub name_cn: Option<String>,
    pub id: i64,
}
