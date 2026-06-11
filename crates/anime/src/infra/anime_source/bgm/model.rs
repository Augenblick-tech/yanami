use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::entity::model::{AnimeEx, AnimeIdType, AnimeLangTarget, AnimeSourceTarget, AnimeTitle};

/// bangumi-data 月份数据通常是一个 JSON 数组，所以解析时你应该使用 `Vec<BangumiItem>`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BangumiItem {
    /// 原始标题
    pub title: String,

    /// 翻译标题。键为语言代码（如 "zh-Hans"），值为该语言的标题数组
    #[serde(default)]
    pub title_translate: HashMap<String, Vec<String>>,

    /// 放送类型，如 "tv", "web", "movie", "ova"
    pub r#type: Option<String>,

    /// 原始语言，如 "ja", "zh-Hans"
    pub lang: Option<String>,

    /// 官方网站
    pub official_site: Option<String>,

    /// 首播时间 (ISO 8601 格式，如 "2026-07-01T00:00:00.000Z")
    pub begin: Option<String>,

    /// 结束时间
    pub end: Option<String>,

    /// 放送周期描述（通常是 ISO 8601 duration 格式）
    pub broadcast: Option<String>,

    /// 该作品在各个站点的放送信息/条目 ID
    #[serde(default)]
    pub sites: Vec<Site>,
}

impl BangumiItem {
    // parse_ex_link
    // 从sites里解析出来bgm和tmdb的关联数据，bgm不允许为空，tmdb允许为空
    pub fn parse_ex_link(&self) -> Result<Vec<AnimeEx>> {
        let i = self;
        let sites = i
            .sites
            .clone()
            .into_iter()
            .filter_map(|s| {
                let id = s.id?;
                Some((s.site, id))
            })
            .collect::<HashMap<String, String>>();
        let mut result = vec![];

        let bgm_id = match sites.get("bangumi") {
            Some(id) => {
                if let Ok(id) = id.parse::<i64>() {
                    id
                } else {
                    return Err(anyhow!("unknown id {}", id));
                }
            }
            None => return Err(anyhow!("not found bangumi")),
        };
        result.push(AnimeEx {
            id: AnimeIdType::Int(bgm_id),
            target: AnimeSourceTarget::Bangumi,
            r#type: self.r#type.clone(),
        });

        if let Some(id) = sites.get("tmdb") {
            let value = id.split('/').collect::<Vec<&str>>();
            if value.len() != 2 && value[0].is_empty() {
                tracing::debug!("unknown {} tmdb site {}", self.title, id);
            } else {
                if let Ok(tmdb_id) = value[1].parse::<i64>() {
                    result.push(AnimeEx {
                        id: AnimeIdType::Int(tmdb_id),
                        target: AnimeSourceTarget::TMDB,
                        r#type: Some(value[0].to_string()),
                    });
                }
            }
        } else {
            tracing::debug!("notfound {} tmdb site", self.title);
        }
        Ok(result)
    }

    pub fn parse_titles(&self) -> Option<Vec<AnimeTitle>> {
        let mut result = vec![];
        for i in &self.title_translate {
            let target = AnimeLangTarget::from(i.0.to_ascii_lowercase().as_str());
            for name in i.1 {
                let match_name = AnimeTitle::to_keywords(name)
                    .into_iter()
                    .collect::<String>();
                result.push(AnimeTitle {
                    name: name.clone(),
                    match_name,
                    target: target.clone(),
                    origin: false,
                });
            }
        }

        Some(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    /// 站点唯一标识符，如 "bilibili", "bangumi", "iqiyi", "aniDB" 等
    pub site: String,

    /// 该作品在该平台上的 ID (因为有些平台使用字符串，有些使用纯数字，推荐用 String 解析)
    pub id: Option<String>,

    /// 有些非结构化的站点不使用 ID，而是直接提供 URL
    pub url: Option<String>,

    /// 在该站点的上线时间
    pub begin: Option<String>,

    /// 在该站点的放送周期
    pub broadcast: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq, Deserialize, Clone, Copy)]
#[serde(into = "u8", try_from = "u8")]
pub enum SubjectType {
    Book,
    Anime,
    Music,
    Game,
    Real,
    Unknown(u8), // 把不认识的数字存起来
}

impl From<u8> for SubjectType {
    fn from(value: u8) -> Self {
        match value {
            1 => SubjectType::Book,
            2 => SubjectType::Anime,
            3 => SubjectType::Music,
            4 => SubjectType::Game,
            6 => SubjectType::Real,
            other => SubjectType::Unknown(other),
        }
    }
}
impl From<SubjectType> for u8 {
    fn from(subject: SubjectType) -> Self {
        match subject {
            SubjectType::Book => 1,
            SubjectType::Anime => 2,
            SubjectType::Music => 3,
            SubjectType::Game => 4,
            SubjectType::Real => 6,
            SubjectType::Unknown(other) => other, // 把存起来的数字原样吐出来
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BangumiSubject {
    pub id: u32,

    /// 条目类型: 1=书籍, 2=动画, 3=音乐, 4=游戏, 6=三次元
    #[serde(rename = "type")]
    pub subject_type: SubjectType,

    pub name: String,

    pub name_cn: Option<String>,
    pub summary: Option<String>,
    pub date: Option<String>,
    pub platform: Option<String>,

    pub nsfw: bool,
    pub locked: bool,

    pub images: Option<Images>,

    #[serde(default)]
    pub infobox: Vec<InfoboxItem>,

    pub volumes: Option<u32>,
    pub eps: Option<u32>,
    pub total_episodes: Option<u32>,

    pub rating: Option<Rating>,
    pub collection: Option<Collection>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl BangumiSubject {
    pub fn parse_infobox(&self) -> HashMap<String, Value> {
        self.infobox
            .clone()
            .into_iter()
            .filter_map(|s| {
                let key = s.key;
                let value = s.value?;
                Some((key, value))
            })
            .collect()
    }

    pub fn parse_titles(&self) -> Vec<AnimeTitle> {
        let mut titles = vec![];
        let match_name = AnimeTitle::to_keywords(&self.name)
            .into_iter()
            .collect::<String>();
        titles.push(AnimeTitle {
            name: self.name.clone(),
            match_name,
            target: AnimeLangTarget::Other("unknown".to_string()),
            origin: true,
        });
        if let Some(name_cn) = &self.name_cn {
            let match_name = AnimeTitle::to_keywords(name_cn)
                .into_iter()
                .collect::<String>();
            titles.push(AnimeTitle {
                name: name_cn.clone(),
                match_name,
                target: AnimeLangTarget::ZhCn,
                origin: false,
            });
        }
        titles
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Images {
    pub large: Option<String>,
    pub common: Option<String>,
    pub medium: Option<String>,
    pub small: Option<String>,
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoboxItem {
    pub key: String,
    /// 可能是 String，也可能是 Array
    pub value: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rating {
    pub rank: Option<u32>, // 排名可能没有（如果评分人数不足）
    pub total: u32,
    pub count: RatingCount,
    pub score: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RatingCount {
    #[serde(rename = "1")]
    pub one: u32,
    #[serde(rename = "2")]
    pub two: u32,
    #[serde(rename = "3")]
    pub three: u32,
    #[serde(rename = "4")]
    pub four: u32,
    #[serde(rename = "5")]
    pub five: u32,
    #[serde(rename = "6")]
    pub six: u32,
    #[serde(rename = "7")]
    pub seven: u32,
    #[serde(rename = "8")]
    pub eight: u32,
    #[serde(rename = "9")]
    pub nine: u32,
    #[serde(rename = "10")]
    pub ten: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Collection {
    pub wish: u32,
    pub collect: u32,
    pub doing: u32,
    pub on_hold: u32,
    pub dropped: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub count: u32,
}

// Bangumi API 返回的 relation 是中文 String，比如 "前传"
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum Relation {
    Prequel,       // 前传
    Sequel,        // 续集
    SideStory,     // 番外篇
    MainStory,     // 主线故事
    SpinOff,       // 衍生
    SameSetting,   // 相同世界观
    Alternative,   // 不同演绎
    Original,      // 原作
    Character,     // 角色出演
    Other(String), // 兜底：Bangumi 经常有奇葩的关系描述(比如"不同世界线")，存在这里
}

impl From<String> for Relation {
    fn from(value: String) -> Self {
        match value.as_str() {
            "前传" => Relation::Prequel,
            "续集" => Relation::Sequel,
            "番外篇" => Relation::SideStory,
            "主线故事" => Relation::MainStory,
            "衍生" => Relation::SpinOff,
            "相同世界观" => Relation::SameSetting,
            "不同演绎" => Relation::Alternative,
            "原作" => Relation::Original,
            "角色出演" => Relation::Character,
            _ => Relation::Other(value),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RelatedSubject {
    pub id: u32,
    #[serde(rename = "type")]
    pub subject_type: SubjectType,
    pub name: String,
    pub name_cn: Option<String>,
    pub images: Option<Images>,
    pub relation: Relation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
    pub data: Option<Vec<T>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// 章节的唯一 ID
    pub id: u32,

    /// 该章节所属的条目 (Subject) ID
    pub subject_id: u32,

    /// 章节类型定义：
    /// 0: 本篇
    /// 1: SP (Special)
    /// 2: OP (Opening)
    /// 3: ED (Ending)
    /// 4: 预告/宣传
    /// 5: MAD
    /// 6: 其他
    #[serde(rename = "type")]
    pub episode_type: u8,

    /// 章节的原名（通常为日文原名，可能未填写）
    pub name: Option<String>,

    /// 章节的中文译名（可能未填写）
    pub name_cn: Option<String>,

    /// 列表排序用数值（存在 SP 如 10.5 的情况，因此是浮点数）
    pub sort: f64,

    /// 集数（多数情况和 sort 相同，但在未定义集数的条目中可能为空）
    pub ep: u32,

    /// 首播日期/发售日期（通常格式为 "YYYY-MM-DD"，可能未填写）
    pub airdate: String,

    /// 章节底下的讨论/回复数量
    pub comment: u32,

    /// 章节的剧情简介（可能未填写）
    pub desc: Option<String>,

    /// 碟片序号（动画多为 0，主要用于音乐专辑分碟）
    pub disc: u32,

    /// 文本格式的时长（如 "24:00"，由用户自由输入，可能未填写）
    pub duration: Option<String>,

    /// 系统计算的纯秒数时长（可能由于 duration 填写不规范导致计算为空）
    pub duration_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchQuery {
    /// 搜索关键词（如 "死神"）
    pub keyword: String,

    /// 排序方式（如 "rank"）
    pub sort: String,

    /// 过滤器配置
    pub filter: FilterConfig,
}

/// 过滤器具体配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterConfig {
    /// 类型筛选列表
    pub r#type: Vec<SubjectType>,

    /// 是否包含敏感内容（NSFW）
    pub nsfw: bool,
}
