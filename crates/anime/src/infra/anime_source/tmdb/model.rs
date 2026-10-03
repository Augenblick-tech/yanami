use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};

/// TMDB 对未定档的条目会把日期返回成空字符串（同一接口也可能返回 null），
/// 空串会让整数日期解析失败并连带整页结果解码失败，这里统一收敛成 None。
fn empty_string_as_none_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(text) if text.trim().is_empty() => Ok(None),
        Some(text) => NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Page<T> {
    /// 当前页码
    pub page: i64,
    /// 搜索结果列表
    pub results: Vec<T>,
    /// 总页数
    pub total_pages: i64,
    /// 总结果数
    pub total_results: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvShowBase {
    /// 是否为成人内容
    pub adult: bool,
    /// 背景图片路径，可能为空
    pub backdrop_path: Option<String>,
    /// 唯一标识符 ID
    pub id: i64,
    /// 制片国家/地区列表
    pub origin_country: Vec<String>,
    /// 原始语言
    pub original_language: String,
    /// 原始名称
    pub original_name: String,
    /// 剧情简介
    pub overview: String,
    /// 流行度/热度分数
    pub popularity: f64,
    /// 海报图片路径，可能为空
    pub poster_path: Option<String>,
    /// 首次开播/上映日期
    #[serde(default, deserialize_with = "empty_string_as_none_date")]
    pub first_air_date: Option<NaiveDate>,
    /// 中文/本地化名称
    pub name: String,
    /// 平均评分
    pub vote_average: f64,
    /// 评分人数计数
    pub vote_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EpisodeBase {
    /// 单集唯一标识符 ID
    pub id: i64,
    /// 单集名称
    pub name: String,
    /// 单集剧情简介
    pub overview: Option<String>,
    /// 单集平均评分
    pub vote_average: f64,
    /// 单集评分人数计数
    pub vote_count: i64,
    /// 单集开播日期
    #[serde(default, deserialize_with = "empty_string_as_none_date")]
    pub air_date: Option<NaiveDate>,
    /// 剧集中的第几集
    pub episode_number: i64,
    /// 生产/制作代码
    pub production_code: String,
    /// 单集片长（分钟），可能为空
    pub runtime: Option<i64>,
    /// 所属第几季
    pub season_number: i64,
    /// 所属剧集唯一标识符 ID
    pub show_id: i64,
    /// 剧照图片路径，可能为空
    pub still_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompanyBase {
    /// 公司/平台唯一标识符 ID
    pub id: i64,
    /// Logo 路径，可能为空
    pub logo_path: Option<String>,
    /// 名称
    pub name: String,
    /// 所属国家
    pub origin_country: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatorBase {
    /// 人员唯一标识符 ID
    pub id: i64,
    /// 演职人员表单项唯一标识符 ID
    pub credit_id: String,
    /// 姓名
    pub name: String,
    /// 性别标识（数字代号）
    pub gender: i64,
    /// 个人头像路径，可能为空
    pub profile_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PersonBase {
    #[serde(flatten)]
    pub creator_base: CreatorBase,
    /// 是否为成人内容演员
    pub adult: bool,
    /// 该人员知名的所属部门
    pub known_for_department: String,
    /// 原始姓名
    pub original_name: String,
    /// 流行度/热度分数
    pub popularity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SeasonBase {
    /// 该季唯一标识符 ID
    pub id: i64,
    /// 该季名称
    pub name: String,
    /// 该季剧情简介
    pub overview: Option<String>,
    /// 该季海报图片路径，可能为空
    pub poster_path: Option<String>,
    /// 第几季
    pub season_number: i64,
    /// 该季平均评分
    pub vote_average: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchTVResult {
    #[serde(flatten)]
    pub inner: TvShowBase,
    /// 题材/类型 ID 列表
    pub genre_ids: Vec<i64>,
    /// 是否为软色情内容
    pub softcore: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvShowDetail {
    #[serde(flatten)]
    pub inner: TvShowBase,
    /// 创作者列表
    pub created_by: Vec<TvShowCreator>,
    /// 单集片长列表
    pub episode_run_time: Vec<i64>,
    /// 题材/类型列表
    pub genres: Vec<TvShowGenre>,
    /// 官方主页链接
    pub homepage: String,
    /// 是否正在制作中
    pub in_production: bool,
    /// 支持的语言代码列表
    pub languages: Vec<String>,
    /// 最后一集开播日期
    #[serde(default, deserialize_with = "empty_string_as_none_date")]
    pub last_air_date: Option<NaiveDate>,
    /// 最近播出的剧集信息，可能为空
    pub last_episode_to_air: Option<EpisodeInfo>,
    /// 下一集播出信息，可能为空
    pub next_episode_to_air: Option<EpisodeInfo>,
    /// 播出网络/平台列表
    pub networks: Vec<TvNetwork>,
    /// 总集数
    pub number_of_episodes: i64,
    /// 总季数
    pub number_of_seasons: i64,
    /// 制作公司列表
    pub production_companies: Vec<ProductionCompany>,
    /// 制片国家详情列表
    pub production_countries: Vec<ProductionCountry>,
    /// 剧集各季详情列表
    pub seasons: Vec<SeasonInfo>,
    /// 剧中使用语言列表
    pub spoken_languages: Vec<SpokenLanguage>,
    /// 剧集状态（如 "Ended", "Returning Series"）
    pub status: String,
    /// 宣传标语/口号
    pub tagline: String,
    /// 剧集类型（如 "Scripted"）
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvShowCreator {
    #[serde(flatten)]
    pub base: CreatorBase,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EpisodeInfo {
    #[serde(flatten)]
    pub base: EpisodeBase,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EpisodeDetail {
    #[serde(flatten)]
    pub inner: EpisodeBase,
    /// 剧集类型（例如 "standard", "finale"）
    pub episode_type: String,
    /// 幕后制作人员/职员列表
    pub crew: Vec<CrewMember>,
    /// 客串演员/明星列表
    pub guest_stars: Vec<GuestStar>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvNetwork {
    #[serde(flatten)]
    pub base: CompanyBase,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductionCompany {
    #[serde(flatten)]
    pub base: CompanyBase,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SeasonInfo {
    #[serde(flatten)]
    pub inner: SeasonBase,
    /// 该季开播日期，可能为空
    #[serde(default, deserialize_with = "empty_string_as_none_date")]
    pub air_date: Option<NaiveDate>,
    /// 该季总集数
    pub episode_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvSeasonDetail {
    #[serde(flatten)]
    pub inner: SeasonBase,
    /// 该季开播日期
    #[serde(default, deserialize_with = "empty_string_as_none_date")]
    pub air_date: Option<NaiveDate>,
    /// 剧集详情列表
    pub episodes: Vec<EpisodeDetail>,
    /// 播出网络/平台列表
    pub networks: Vec<TvNetwork>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrewMember {
    #[serde(flatten)]
    pub base: PersonBase,
    /// 部门名称
    pub department: String,
    /// 具体职务名称
    pub job: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GuestStar {
    #[serde(flatten)]
    pub base: PersonBase,
    /// 饰演的角色名称
    pub character: String,
    /// 演员在演员表中的排序权重
    pub order: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SeriesAlternativeTitles {
    /// 媒体资源唯一标识符 ID
    pub id: i64,
    /// 别名或译名结果列表
    pub results: Vec<AlternativeTitle>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlternativeTitle {
    /// 地区或国家代码
    pub iso_3166_1: String,
    /// 对应的电影/剧集名称、译名或别名
    pub title: String,
    /// 标题类型
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvShowGenre {
    /// 类型唯一标识符 ID
    pub id: i64,
    /// 类型名称
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductionCountry {
    /// 国家代码
    pub iso_3166_1: String,
    /// 国家名称
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpokenLanguage {
    /// 语言英文名称
    pub english_name: String,
    /// 语言代码
    pub iso_639_1: String,
    /// 语言本地名称
    pub name: String,
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    /// 真实抓取：GET /3/search/tv?query=世界最強の魔女、始めました&language=zh-CN
    /// 该条目尚未定档，TMDB 把 first_air_date 返回成空字符串。
    const UNANNOUNCED_SEARCH_JSON: &str = r#"{"page":1,"results":[{"adult":false,"backdrop_path":"/bM9pGax5PQCvipy7zifscXsVJ8Z.jpg","genre_ids":[16,35,10759,10765],"id":321518,"origin_country":["JP"],"original_language":"ja","original_name":"世界最強の魔女、始めました","overview":"","popularity":1.1531,"poster_path":"/pw9Ha86plPe344JED6ij85UPZKZ.jpg","first_air_date":"","softcore":false,"name":"世界最强魔女，始动","vote_average":0.0,"vote_count":0}],"total_pages":1,"total_results":1}"#;

    /// 真实抓取：GET /3/tv/321518?language=zh-CN（同一未定档条目）
    const UNANNOUNCED_DETAIL_JSON: &str = r#"{"adult":false,"backdrop_path":"/bM9pGax5PQCvipy7zifscXsVJ8Z.jpg","created_by":[],"episode_run_time":[],"first_air_date":"","genres":[{"id":16,"name":"动画"},{"id":35,"name":"喜剧"},{"id":10759,"name":"动作冒险"},{"id":10765,"name":"Sci-Fi & Fantasy"}],"homepage":"https://sekamajo-anime.com","id":321518,"in_production":true,"languages":["ja"],"last_air_date":null,"last_episode_to_air":null,"name":"世界最强魔女，始动","next_episode_to_air":null,"networks":[],"number_of_episodes":1,"number_of_seasons":1,"origin_country":["JP"],"original_language":"ja","original_name":"世界最強の魔女、始めました","overview":"","popularity":1.2985,"poster_path":"/pw9Ha86plPe344JED6ij85UPZKZ.jpg","production_companies":[{"id":43693,"logo_path":"/aO5hWIoTnGjMnVQF7JrlnJgIEvD.png","name":"Bridge","origin_country":"JP"},{"id":279191,"logo_path":null,"name":"AISLE","origin_country":"JP"}],"production_countries":[{"iso_3166_1":"JP","name":"Japan"}],"seasons":[{"air_date":null,"episode_count":1,"id":514478,"name":"第 1 季","overview":"","poster_path":"/2Qshz88lEA1N9Ba3h8zFyu8omRJ.jpg","season_number":1,"vote_average":0.0}],"softcore":false,"spoken_languages":[{"english_name":"Japanese","iso_639_1":"ja","name":"日本語"}],"status":"In Production","tagline":"","type":"Scripted","vote_average":0.0,"vote_count":0}"#;

    /// 真实抓取：GET /3/tv/321518/season/1?language=zh-CN
    const UNANNOUNCED_SEASON_JSON: &str = r#"{"_id":"69f7d51476c0eece54326d43","air_date":null,"episodes":[{"air_date":null,"episode_number":1,"episode_type":"standard","id":7228273,"name":"第 1 集","overview":"","production_code":"","runtime":null,"season_number":1,"show_id":321518,"still_path":null,"vote_average":0.0,"vote_count":0,"crew":[],"guest_stars":[]}],"name":"第 1 季","networks":[],"overview":"","id":514478,"poster_path":"/2Qshz88lEA1N9Ba3h8zFyu8omRJ.jpg","season_number":1,"vote_average":0.0}"#;

    #[test]
    fn search_page_with_empty_first_air_date_decodes_to_none() {
        let page: Page<SearchTVResult> = serde_json::from_str(UNANNOUNCED_SEARCH_JSON)
            .expect("real tmdb search body must decode");

        assert_eq!(page.total_results, 1);
        assert_eq!(page.results[0].inner.id, 321518);
        assert_eq!(page.results[0].inner.name, "世界最强魔女，始动");
        assert_eq!(page.results[0].inner.first_air_date, None);
    }

    #[test]
    fn tv_detail_with_empty_first_air_date_decodes_to_none() {
        let detail: TvShowDetail = serde_json::from_str(UNANNOUNCED_DETAIL_JSON)
            .expect("real tmdb detail body must decode");

        assert_eq!(detail.inner.id, 321518);
        assert_eq!(detail.inner.first_air_date, None);
        assert_eq!(detail.last_air_date, None);
        assert_eq!(detail.seasons[0].air_date, None);
        assert_eq!(detail.seasons[0].inner.season_number, 1);
    }

    #[test]
    fn season_detail_with_null_air_date_decodes_to_none() {
        let season: TvSeasonDetail = serde_json::from_str(UNANNOUNCED_SEASON_JSON)
            .expect("real tmdb season body must decode");

        assert_eq!(season.air_date, None);
        assert_eq!(season.episodes[0].inner.episode_number, 1);
        assert_eq!(season.episodes[0].inner.air_date, None);
    }

    #[test]
    fn season_detail_with_empty_air_date_decodes_to_none() {
        // 同一批未定档日期，TMDB 另一种编码就是空字符串：把真实抓取体的 null 换成空串
        let body = UNANNOUNCED_SEASON_JSON.replace("\"air_date\":null", "\"air_date\":\"\"");
        assert!(
            body.contains("\"air_date\":\"\""),
            "fixture must contain empty date"
        );

        let season: TvSeasonDetail =
            serde_json::from_str(&body).expect("empty string date must decode");

        assert_eq!(season.air_date, None);
        assert_eq!(season.episodes[0].inner.air_date, None);
    }

    #[test]
    fn valid_first_air_date_still_decodes_to_some() {
        let body = r#"{"adult":false,"backdrop_path":null,"id":65942,"origin_country":["JP"],
            "original_language":"ja","original_name":"Re:ゼロから始める異世界生活","overview":"",
            "popularity":1.0,"poster_path":null,"first_air_date":"2016-04-04",
            "name":"Re：从零开始的异世界生活","vote_average":8.0,"vote_count":10}"#;

        let base: TvShowBase = serde_json::from_str(body).expect("valid date must still decode");

        assert_eq!(
            base.first_air_date,
            Some(NaiveDate::from_ymd_opt(2016, 4, 4).expect("date must be valid"))
        );
    }
}
