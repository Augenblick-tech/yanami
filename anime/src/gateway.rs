#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiCalendar {
    pub weekday: BangumiWeekday,
    pub items: Vec<BangumiCalendarItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiWeekday {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiCalendarItem {
    pub id: i64,
    pub name: String,
    pub air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiSubject {
    pub name: Option<String>,
    pub name_cn: Option<String>,
    pub air_date: Option<String>,
    pub eps: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiCalendarAnime {
    pub id: i64,
    pub name: String,
    pub weekday: i64,
    pub eps: Option<i64>,
    pub air_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiAnimeSearchResultPage {
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
    pub data: Vec<BangumiSubjectSmall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangumiSubjectSmall {
    pub id: i64,
    pub subject_type: i64,
    pub name: String,
    pub name_cn: String,
    pub summary: String,
    pub air_date: Option<String>,
    pub air_weekday: Option<i64>,
    pub eps: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbSearchResult {
    pub results: Vec<TmdbSearchResultItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbSearchResultItem {
    pub id: i64,
    pub name: Option<String>,
    pub original_language: Option<String>,
    pub first_air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbSeriesDetails {
    pub first_air_date: Option<String>,
    pub name: Option<String>,
    pub seasons: Vec<TmdbSeason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbSeason {
    pub episode_count: i64,
    pub season_number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbAlternativeTitles {
    pub results: Vec<TmdbAlternativeTitleItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdbAlternativeTitleItem {
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YucLink {
    pub label: String,
    pub href: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YucLabeledValue {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YucSeasonCalendarPage {
    pub page_title: String,
    pub page_url: String,
    pub season_code: String,
    pub updated_at: Option<String>,
    pub entries: Vec<YucAnimeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YucAnimeEntry {
    pub title_zh: String,
    pub title_original: Option<String>,
    pub image_url: Option<String>,
    pub source_type: Option<String>,
    pub genre_text: Option<String>,
    pub original_work: Option<String>,
    pub original_work_extra: Option<String>,
    pub schedule_weekday: Option<String>,
    pub schedule_time: Option<String>,
    pub schedule_start_date: Option<String>,
    pub schedule_note: Option<String>,
    pub broadcast_text: Option<String>,
    pub streaming_links: Vec<YucLink>,
    pub resource_links: Vec<YucLink>,
    pub staff: Vec<YucLabeledValue>,
    pub cast: Vec<String>,
}
