use crate::entity::model::{AnimeBaseData, AnimeIdType, AnimeMetadata, AnimeSourceTarget};

#[derive(Clone, Debug)]
pub struct AnimeEntity {
    data: AnimeBaseData,
}

impl AnimeEntity {
    pub(crate) fn new(data: AnimeBaseData) -> Self {
        Self { data }
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    /// 系列身份只有一个出处：该条目的 TMDB 剧集 id。
    pub fn series_id(&self) -> Option<i64> {
        self.data
            .metadata
            .external_link
            .iter()
            .find(|i| i.target == AnimeSourceTarget::TMDB)
            .and_then(|i| match &i.id {
                AnimeIdType::Int(id) if *id > 0 => Some(*id),
                _ => None,
            })
    }

    pub fn title(&self) -> Option<&str> {
        self.data
            .metadata
            .titles
            .iter()
            .find(|i| i.origin)
            .map(|i| i.name.as_str())
    }

    /// 媒体库季号只有一个出处：TMDB 的季度编号。取不到就是落点未定，不猜。
    pub fn season_number(&self) -> Option<u32> {
        self.data
            .metadata
            .season
            .iter()
            .find(|i| i.target == AnimeSourceTarget::TMDB)
            .map(|i| i.season)
            .filter(|i| *i > 0)
    }

    pub fn lock(&mut self) {
        self.data.lock = true;
    }

    pub fn unlock(&mut self) {
        self.data.lock = false;
    }

    pub fn metadata(&self) -> &AnimeMetadata {
        &self.data.metadata
    }

    pub(crate) fn is_locked(&self) -> bool {
        self.data.lock
    }

    pub fn update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.data.lock && !self.data.metadata.eq(metadata) {
            self.data.metadata = metadata.clone();
        }
    }

    pub fn force_update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.data.metadata.eq(metadata) {
            self.data.metadata = metadata.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeSeason, AnimeSourceTarget,
        AnimeTitle,
    };

    fn air_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()
    }

    fn build(
        external_link: Vec<AnimeEx>,
        titles: Vec<AnimeTitle>,
        season: Vec<AnimeSeason>,
    ) -> AnimeEntity {
        AnimeEntity::new(AnimeBaseData {
            id: 42,
            metadata: AnimeMetadata {
                series_metadata: None,
                external_link,
                titles,
                air_weekday: AnimeAirWeekday::Wednesday,
                air_date: air_date(),
                air_quarter: 202607,
                season,
            },
            lock: false,
        })
    }

    fn title(name: &str, origin: bool) -> AnimeTitle {
        AnimeTitle {
            name: name.to_string(),
            match_name: AnimeTitle::to_keywords(name).into_iter().collect(),
            target: AnimeLangTarget::JP,
            origin,
        }
    }

    fn tmdb_link(id: AnimeIdType) -> AnimeEx {
        AnimeEx {
            id,
            target: AnimeSourceTarget::TMDB,
            r#type: Some("tv".to_string()),
        }
    }

    fn bangumi_link(id: i64) -> AnimeEx {
        AnimeEx {
            id: AnimeIdType::Int(id),
            target: AnimeSourceTarget::Bangumi,
            r#type: Some("tv".to_string()),
        }
    }

    fn season(target: AnimeSourceTarget, number: u32) -> AnimeSeason {
        AnimeSeason {
            target,
            lang: AnimeLangTarget::JP,
            desc: String::new(),
            season: number,
            eps: Vec::new(),
            planned_episode_count: 12,
        }
    }

    #[test]
    fn series_id_only_accepts_positive_tmdb_integer_identity() {
        let entity = build(
            vec![bangumi_link(558064), tmdb_link(AnimeIdType::Int(286346))],
            vec![title("女主角？圣女？", true)],
            vec![],
        );
        assert_eq!(entity.series_id(), Some(286346));

        // 没有 TMDB 链接时没有系列身份
        let bangumi_only = build(vec![bangumi_link(558064)], vec![], vec![]);
        assert_eq!(bangumi_only.series_id(), None);

        // TMDB 身份不是正整数时也不认
        let string_id = build(
            vec![tmdb_link(AnimeIdType::String("286346".to_string()))],
            vec![],
            vec![],
        );
        assert_eq!(string_id.series_id(), None);

        let zero_id = build(vec![tmdb_link(AnimeIdType::Int(0))], vec![], vec![]);
        assert_eq!(zero_id.series_id(), None);
    }

    #[test]
    fn season_number_ignores_non_tmdb_and_zero_seasons() {
        let absent = build(vec![], vec![], vec![]);
        assert_eq!(absent.season_number(), None);

        // 其他来源的季度不算媒体库季号
        let bangumi_season = build(vec![], vec![], vec![season(AnimeSourceTarget::Bangumi, 3)]);
        assert_eq!(bangumi_season.season_number(), None);

        // 第 0 季（特典）不产生落点季号
        let special = build(vec![], vec![], vec![season(AnimeSourceTarget::TMDB, 0)]);
        assert_eq!(special.season_number(), None);

        let second = build(vec![], vec![], vec![season(AnimeSourceTarget::TMDB, 2)]);
        assert_eq!(second.season_number(), Some(2));
    }

    #[test]
    fn title_returns_first_origin_title_only() {
        let entity = build(
            vec![],
            vec![
                title("女主角", false),
                title("ヒロイン", true),
                title("Heroine", true),
            ],
            vec![],
        );
        assert_eq!(entity.title(), Some("ヒロイン"));

        // 只有译名时没有原始标题
        let translated_only = build(vec![], vec![title("女主角", false)], vec![]);
        assert_eq!(translated_only.title(), None);
    }
}
