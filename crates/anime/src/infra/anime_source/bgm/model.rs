use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::entity::model::{
    AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeSourceTarget, AnimeTitle,
};

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

    /// 放送星期，没写就是 None
    pub fn parse_air_weekday(&self) -> Option<AnimeAirWeekday> {
        self.infobox
            .iter()
            .find(|i| i.key == "放送星期")
            .and_then(|i| i.value.as_ref())
            .and_then(|v| v.as_str())
            .and_then(|v| AnimeAirWeekday::try_from(v).ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 数据来源：https://raw.githubusercontent.com/bangumi-data/bangumi-data/refs/heads/master/data/items/2026/07.json
    /// 该文件中「ヒロイン？聖女？いいえ、オールワークスメイドです(誇)！」条目原文，仅保留测试用到的字段，取值未改动。
    const BANGUMI_ITEM_JSON: &str = r#"{
        "title": "ヒロイン？聖女？いいえ、オールワークスメイドです(誇)！",
        "titleTranslate": {
            "zh-Hans": ["女主角？圣女？不，我是杂役女仆（自豪）！"],
            "zh-Hant": ["女主角？聖女？都不對，我是雜役女僕（自豪）！"],
            "en": ["Heroine? Saint? No, I'm an All-Works Maid (and Proud of It)!"]
        },
        "type": "tv",
        "lang": "ja",
        "officialSite": "https://all-works-maid-anime.com/",
        "begin": "2026-07-01T13:00:00.000Z",
        "sites": [
            { "site": "bangumi", "id": "558064" },
            { "site": "tmdb", "id": "tv/286346" }
        ]
    }"#;

    /// 数据来源：https://api.bgm.tv/v0/subjects/8 （コードギアス 反逆のルルーシュR2）
    /// 仅保留测试断言用到的字段，取值未改动（infobox 的「别名」在真实数据里更长，这里只留一个元素）。
    const BANGUMI_SUBJECT_JSON: &str = r#"{
        "id": 8,
        "type": 2,
        "name": "コードギアス 反逆のルルーシュR2",
        "name_cn": "Code Geass 反叛的鲁路修R2",
        "date": "2008-04-06",
        "platform": "TV",
        "eps": 25,
        "total_episodes": 25,
        "nsfw": false,
        "locked": false,
        "infobox": [
            { "key": "中文名", "value": "Code Geass 反叛的鲁路修R2" },
            { "key": "别名", "value": [{ "v": "叛逆的鲁鲁修R2" }] },
            { "key": "话数", "value": "25" },
            { "key": "放送开始", "value": "2008年4月6日" }
        ]
    }"#;

    fn item() -> BangumiItem {
        serde_json::from_str(BANGUMI_ITEM_JSON).unwrap()
    }

    fn subject() -> BangumiSubject {
        serde_json::from_str(BANGUMI_SUBJECT_JSON).unwrap()
    }

    #[test]
    fn bangumi_item_parse_ex_link_reads_bangumi_and_tmdb() {
        let links = item().parse_ex_link().unwrap();
        assert_eq!(links.len(), 2);

        assert_eq!(links[0].target, AnimeSourceTarget::Bangumi);
        assert_eq!(links[0].id, AnimeIdType::Int(558064));
        assert_eq!(links[0].r#type.as_deref(), Some("tv"));

        assert_eq!(links[1].target, AnimeSourceTarget::TMDB);
        assert_eq!(links[1].id, AnimeIdType::Int(286346));
        // tmdb 站点的 id 形如 "tv/286346"，前缀被当作类型保留
        assert_eq!(links[1].r#type.as_deref(), Some("tv"));
    }

    #[test]
    fn bangumi_item_parse_ex_link_requires_bangumi_site() {
        let mut value: Value = serde_json::from_str(BANGUMI_ITEM_JSON).unwrap();
        value["sites"] = serde_json::json!([{ "site": "tmdb", "id": "tv/286346" }]);
        let item: BangumiItem = serde_json::from_value(value).unwrap();

        let err = item
            .parse_ex_link()
            .expect_err("missing bangumi site should fail");
        assert_eq!(err.to_string(), "not found bangumi");
    }

    #[test]
    fn bangumi_item_parse_ex_link_requires_numeric_bangumi_id() {
        let mut value: Value = serde_json::from_str(BANGUMI_ITEM_JSON).unwrap();
        value["sites"] = serde_json::json!([{ "site": "bangumi", "id": "abc" }]);
        let item: BangumiItem = serde_json::from_value(value).unwrap();

        let err = item
            .parse_ex_link()
            .expect_err("non-numeric id should fail");
        assert_eq!(err.to_string(), "unknown id abc");
    }

    #[test]
    fn bangumi_item_parse_ex_link_panics_on_tmdb_id_without_type_prefix() {
        // 由真实条目派生：把 tmdb 的 "tv/286346" 改成不带类型前缀的 "286346"。
        let mut value: Value = serde_json::from_str(BANGUMI_ITEM_JSON).unwrap();
        value["sites"][1]["id"] = Value::String("286346".to_string());
        let item: BangumiItem = serde_json::from_value(value).unwrap();

        // 已知缺陷：model.rs 的 `value.len() != 2 && value[0].is_empty()` 应为 `||`，
        // 使得缺少类型前缀的 tmdb id 会走到 value[1] 越界 panic。这里只断言现状，不修复。
        // 触发时临时静音 panic 输出，避免这条预期中的 panic 干扰测试报告。
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| item.parse_ex_link()));
        std::panic::set_hook(previous);

        let payload = result.expect_err("known defect currently panics");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            message.contains("index out of bounds"),
            "unexpected panic payload: {message}"
        );
    }

    #[test]
    fn bangumi_item_parse_titles_translates_every_language() {
        let titles = item().parse_titles().unwrap();
        assert_eq!(titles.len(), 3);
        assert!(titles.iter().all(|t| !t.origin));

        let en = titles
            .iter()
            .find(|t| t.target == AnimeLangTarget::EN)
            .expect("missing en title");
        assert_eq!(
            en.name,
            "Heroine? Saint? No, I'm an All-Works Maid (and Proud of It)!"
        );
        assert_eq!(en.match_name, "heroinesaintnoimanallworksmaidandproudofit");

        let zh_cn = titles
            .iter()
            .find(|t| t.target == AnimeLangTarget::ZhCn)
            .expect("missing zh-Hans title");
        assert_eq!(zh_cn.name, "女主角？圣女？不，我是杂役女仆（自豪）！");
        assert_eq!(zh_cn.match_name, "女主角圣女不我是杂役女仆自豪");

        let zh_tw = titles
            .iter()
            .find(|t| t.target == AnimeLangTarget::ZhTw)
            .expect("missing zh-Hant title");
        assert_eq!(zh_tw.name, "女主角？聖女？都不對，我是雜役女僕（自豪）！");
    }

    #[test]
    fn bangumi_subject_parses_subject_type_and_names() {
        let subject = subject();
        assert_eq!(subject.id, 8);
        assert_eq!(subject.subject_type, SubjectType::Anime);
        assert_eq!(subject.name, "コードギアス 反逆のルルーシュR2");
        assert_eq!(
            subject.name_cn.as_deref(),
            Some("Code Geass 反叛的鲁路修R2")
        );
        assert_eq!(subject.eps, Some(25));
    }

    #[test]
    fn bangumi_subject_parse_titles_marks_origin_and_cn() {
        let titles = subject().parse_titles();
        assert_eq!(titles.len(), 2);

        assert!(titles[0].origin);
        assert_eq!(titles[0].name, "コードギアス 反逆のルルーシュR2");
        assert_eq!(
            titles[0].target,
            AnimeLangTarget::Other("unknown".to_string())
        );
        assert_eq!(titles[0].match_name, "コードギアス反逆のルルーシュr2");

        assert!(!titles[1].origin);
        assert_eq!(titles[1].name, "Code Geass 反叛的鲁路修R2");
        assert_eq!(titles[1].target, AnimeLangTarget::ZhCn);
        assert_eq!(titles[1].match_name, "codegeass反叛的鲁路修r2");
    }

    #[test]
    fn bangumi_subject_parse_titles_without_cn_name() {
        let mut subject = subject();
        subject.name_cn = None;

        let titles = subject.parse_titles();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].origin);
    }

    #[test]
    fn bangumi_subject_parse_infobox_keeps_raw_values_and_skips_empty() {
        let mut subject = subject();
        let infobox = subject.parse_infobox();
        assert_eq!(infobox.len(), 4);

        // 话数在真实数据里是字符串而不是数字
        assert_eq!(infobox["话数"].as_str(), Some("25"));
        assert!(infobox["话数"].as_u64().is_none());
        // 别名是对象数组
        assert_eq!(infobox["别名"][0]["v"], "叛逆的鲁鲁修R2");
        assert_eq!(infobox["放送开始"], "2008年4月6日");

        // 额外塞一条 value 为 null 的条目，生产逻辑会把它过滤掉
        subject.infobox.push(InfoboxItem {
            key: "空值".to_string(),
            value: None,
        });
        let infobox = subject.parse_infobox();
        assert_eq!(infobox.len(), 4);
        assert!(!infobox.contains_key("空值"));
    }

    #[test]
    fn parse_air_weekday_is_none_when_the_subject_has_no_weekday() {
        // 真实抓取的 subject 8 的 infobox 里没有放送星期
        assert_eq!(subject().parse_air_weekday(), None);
    }

    #[test]
    fn parse_air_weekday_reads_the_weekday_from_the_infobox() {
        // 放送星期是 infobox 里的一项，取值形如「星期五」
        let mut subject = subject();
        subject.infobox.push(InfoboxItem {
            key: "放送星期".to_string(),
            value: Some(serde_json::json!("星期五")),
        });

        assert_eq!(subject.parse_air_weekday(), Some(AnimeAirWeekday::Friday));
    }

    #[test]
    fn parse_air_weekday_is_none_when_the_infobox_value_is_not_a_weekday() {
        let mut subject = subject();
        subject.infobox.push(InfoboxItem {
            key: "放送星期".to_string(),
            value: Some(serde_json::json!("每天")),
        });

        assert_eq!(subject.parse_air_weekday(), None);
    }
}
