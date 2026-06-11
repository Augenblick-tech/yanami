use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

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
    pub air_date: Option<NaiveDate>,
    /// 该季总集数
    pub episode_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TvSeasonDetail {
    #[serde(flatten)]
    pub inner: SeasonBase,
    /// 该季开播日期
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
