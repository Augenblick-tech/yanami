use chrono::{Duration, Months, NaiveDateTime};
use common::shared::{cap::FeedSearchUrlProvider, error::Error, model::SearchUrls};

use crate::entity::model::{
    ClaimResult::{self},
    Episode, SubAnimeBaseData, SubAnimeExtendData,
    SubAnimeSearchStatus::{self},
    SubAnimeStatus,
};

pub trait SubAnimeEntityMatcher {
    fn try_claim(&mut self) -> ClaimResult;
    fn request_search(&mut self) -> bool;
    fn match_time_range(&self) -> std::ops::Range<NaiveDateTime>;
}

#[derive(Clone)]
pub struct SubAnimeEntity {
    data: SubAnimeBaseData,
    extend: SubAnimeExtendData,
}

impl SubAnimeEntity {
    pub(super) fn new(data: SubAnimeBaseData, extend: SubAnimeExtendData) -> Self {
        Self { data, extend }
    }

    pub(super) fn get_base_data(&self) -> &SubAnimeBaseData {
        &self.data
    }

    pub(super) fn get_rule_id(&self) -> Option<i64> {
        self.data.rule_id
    }

    pub(super) fn auto_bind_rule(&mut self, rule_id: i64) -> Result<(), Error> {
        if let Some(id) = self.data.rule_id {
            if id == rule_id {
                Ok(())
            } else {
                Err(Error::conflict(
                    "sub anime entity auto binding rule failed, rule alreay binded",
                ))
            }
        } else {
            self.data.rule_id = Some(rule_id);
            Ok(())
        }
    }

    pub(super) fn update_progress(&mut self, eps: &[Episode]) {
        // 总集数为 0 时这一季没有可数的集数，进了多少集都不算它的进度
        if self.extend.eps == 0 {
            return;
        }
        let mut eps_numbers = eps.iter().filter_map(|i| i.ep_num).collect::<Vec<_>>();
        eps_numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        eps_numbers.dedup();
        self.data.progress = eps_numbers.len() as u32;
    }

    pub fn space_id(&self) -> i64 {
        self.data.space_id
    }

    pub(super) fn keywords(&self) -> Vec<String> {
        self.extend
            .titles
            .iter()
            .map(|i| common::shared::str::nfkc_to_lowercase(i))
            .collect()
    }

    pub(super) fn eps_number(&self) -> u32 {
        self.extend.eps
    }
}

impl SubAnimeEntity {
    pub fn sub_status(&self) -> SubAnimeStatus {
        if self.is_completed() {
            SubAnimeStatus::Completed
        } else {
            SubAnimeStatus::Enable
        }
    }

    // 总集数为 0 时这一季还没有可数的集数，不算完结，仍可参与匹配
    pub fn is_completed(&self) -> bool {
        self.extend.eps > 0 && self.data.progress >= self.extend.eps
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn anime_id(&self) -> i64 {
        self.data.anime_id
    }

    pub fn get_binding_rule_name(&self) -> Option<&str> {
        if let Some(name) = &self.extend.rule_name {
            Some(name)
        } else {
            None
        }
    }

    pub fn progress(&self) -> u32 {
        self.data.progress
    }

    pub fn search_status(&self) -> SubAnimeSearchStatus {
        self.data.search_status
    }

    pub fn enable_search(&mut self) -> bool {
        if self.data.search_status == SubAnimeSearchStatus::NotSearch {
            self.data.search_status = SubAnimeSearchStatus::Pending;
            true
        } else {
            false
        }
    }

    pub fn cancel_search(&mut self) -> bool {
        if self.data.search_status != SubAnimeSearchStatus::NotSearch {
            self.data.search_status = SubAnimeSearchStatus::NotSearch;
            true
        } else {
            false
        }
    }

    pub(super) fn search_keywords(&self) -> Vec<String> {
        let mut keywords: Vec<String> = self
            .extend
            .titles
            .iter()
            .flat_map(|i| common::shared::str::to_search_keywords(i))
            .collect();
        keywords.sort();
        keywords.dedup();
        keywords
    }

    pub fn get_search_urls(
        &self,
        search_url_provider: &dyn FeedSearchUrlProvider,
    ) -> Vec<SearchUrls> {
        let keywords = self.search_keywords();
        search_url_provider.made_search_url(&keywords)
    }
}

impl SubAnimeEntityMatcher for SubAnimeEntity {
    // try_claim 确认是否可进行匹配
    fn try_claim(&mut self) -> ClaimResult {
        use ClaimResult::*;
        use SubAnimeSearchStatus::*;
        if self.is_completed() {
            self.cancel_search();
            return Completed;
        }

        if self.data.search_status == Pending {
            self.data.search_status = Matching;
            return Matched;
        }
        AlreayMartched
    }

    // 确认是否需要进行搜索
    // 需要搜索时把订阅重置为不搜索：搜索中不存订阅表，由搜索委托上的记录算出来
    fn request_search(&mut self) -> bool {
        use SubAnimeSearchStatus::*;
        if self.is_completed() {
            self.cancel_search();
            return false;
        }

        if self.data.search_status == Matching || self.data.search_status == Searching {
            self.data.search_status = NotSearch;
            true
        } else {
            false
        }
    }

    /// 获取本地匹配的时间范围
    ///
    /// 时间段 = [air_date - 1个月, air_date + 更新周期 + 3个月]
    /// 其中更新周期 = (总集数 - 1) * 7 天（按每周一集计算）
    /// 结果使用utc时间，即0时区
    fn match_time_range(&self) -> std::ops::Range<NaiveDateTime> {
        let start = self
            .extend
            .air_date
            .checked_sub_months(Months::new(1))
            .unwrap_or(self.extend.air_date)
            .and_hms_opt(0, 0, 0)
            .unwrap();

        // 更新周期天数
        let update_days = if self.extend.eps > 0 {
            (self.extend.eps - 1) as i64 * 7
        } else {
            0
        };
        let update_duration = Duration::days(update_days);

        // 结束日期 = air_date + 更新周期 + 3个月
        let after_update = self.extend.air_date + update_duration;
        let end = after_update
            .checked_add_months(Months::new(3))
            .unwrap_or(after_update)
            .and_hms_opt(0, 0, 0)
            .unwrap();

        start..end
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common::shared::{cap::FeedSearchUrlProvider, model::SearchUrls};

    use super::{SubAnimeEntity, SubAnimeEntityMatcher};
    use crate::entity::model::{
        ClaimResult, Episode, EpsiodeStatus, SubAnimeBaseData, SubAnimeExtendData,
        SubAnimeSearchStatus, SubAnimeStatus,
    };

    fn entity(status: SubAnimeSearchStatus, progress: u32, eps: u32) -> SubAnimeEntity {
        let air_date = NaiveDate::from_ymd_opt(2024, 4, 1).expect("air date 2024-04-01 is valid");
        SubAnimeEntity::new(
            SubAnimeBaseData {
                id: 1,
                anime_id: 100,
                space_id: 9,
                rule_id: None,
                search_status: status,
                progress,
            },
            SubAnimeExtendData {
                eps,
                rule_name: None,
                titles: vec!["某番".to_string()],
                air_date,
            },
        )
    }

    fn episode(ep_num: Option<f64>, byte: u8) -> Episode {
        Episode {
            sub_anime_id: 1,
            resource_id: [byte; 20],
            status: EpsiodeStatus::Pending,
            ep_num,
        }
    }

    #[test]
    fn try_claim_completed_cancels_search() {
        let mut e = entity(SubAnimeSearchStatus::Searching, 12, 12);
        // ClaimResult 没有实现 PartialEq，用 matches! 逐个变体断言
        assert!(matches!(e.try_claim(), ClaimResult::Completed));
        // 已完结时把搜索状态复位为未搜索
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn try_claim_pending_transitions_to_matching() {
        let mut e = entity(SubAnimeSearchStatus::Pending, 0, 12);
        assert!(matches!(e.try_claim(), ClaimResult::Matched));
        assert_eq!(e.search_status(), SubAnimeSearchStatus::Matching);
    }

    #[test]
    fn try_claim_non_pending_is_already_matched() {
        for status in [
            SubAnimeSearchStatus::NotSearch,
            SubAnimeSearchStatus::Matching,
            SubAnimeSearchStatus::Searching,
        ] {
            let mut e = entity(status, 0, 12);
            assert!(matches!(e.try_claim(), ClaimResult::AlreayMartched));
            // 非 Pending 时不改动状态
            assert_eq!(e.search_status(), status);
        }
    }

    #[test]
    fn try_claim_zero_eps_sub_anime_still_waits_for_local_match() {
        let mut e = entity(SubAnimeSearchStatus::Pending, 0, 0);
        // 总集数为 0 不算完结，照常进入本地匹配
        assert!(matches!(e.try_claim(), ClaimResult::Matched));
        assert_eq!(e.search_status(), SubAnimeSearchStatus::Matching);
    }

    #[test]
    fn zero_eps_sub_anime_is_not_completed() {
        let e = entity(SubAnimeSearchStatus::NotSearch, 0, 0);
        assert!(!e.is_completed());
        assert_eq!(e.sub_status(), SubAnimeStatus::Enable);
    }

    #[test]
    fn request_search_from_matching_asks_for_search() {
        let mut e = entity(SubAnimeSearchStatus::Matching, 0, 12);
        assert!(e.request_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn request_search_from_searching_asks_for_search() {
        let mut e = entity(SubAnimeSearchStatus::Searching, 0, 12);
        assert!(e.request_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn request_search_from_pending_is_false() {
        let mut e = entity(SubAnimeSearchStatus::Pending, 0, 12);
        assert!(!e.request_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::Pending);
    }

    #[test]
    fn request_search_from_not_search_is_false() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        assert!(!e.request_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn request_search_completed_cancels_search() {
        let mut e = entity(SubAnimeSearchStatus::Searching, 12, 12);
        assert!(!e.request_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn match_time_range_spans_one_month_before_to_update_plus_three_months() {
        let e = entity(SubAnimeSearchStatus::NotSearch, 0, 3);
        let start = NaiveDate::from_ymd_opt(2024, 3, 1)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        // 更新周期 = (3 - 1) * 7 = 14 天 → 2024-04-15，再 +3 个月 → 2024-07-15
        let end = NaiveDate::from_ymd_opt(2024, 7, 15)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        assert_eq!(e.match_time_range(), start..end);
    }

    #[test]
    fn match_time_range_zero_eps_has_no_update_period() {
        let e = entity(SubAnimeSearchStatus::NotSearch, 0, 0);
        let start = NaiveDate::from_ymd_opt(2024, 3, 1)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        let end = NaiveDate::from_ymd_opt(2024, 7, 1)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        assert_eq!(e.match_time_range(), start..end);
    }

    #[test]
    fn update_progress_empty_slice_is_zero() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 7, 12);
        e.update_progress(&[]);
        assert_eq!(e.progress(), 0);
    }

    #[test]
    fn update_progress_does_not_count_episodes_when_eps_is_zero() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 0);
        e.update_progress(&[episode(Some(1.0), 1), episode(Some(2.0), 2)]);
        assert_eq!(e.progress(), 0);
    }

    #[test]
    fn update_progress_ignores_episodes_without_number() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.update_progress(&[episode(None, 1), episode(None, 2)]);
        assert_eq!(e.progress(), 0);
    }

    #[test]
    fn update_progress_dedups_same_episode_number() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.update_progress(&[
            episode(Some(1.0), 1),
            episode(Some(2.0), 2),
            episode(Some(2.0), 3),
            episode(Some(3.0), 4),
        ]);
        // 四个剧集里只有 1.0 / 2.0 / 3.0 三个不同集号
        assert_eq!(e.progress(), 3);
    }

    #[test]
    fn update_progress_handles_unsorted_numbers() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.update_progress(&[
            episode(Some(3.0), 1),
            episode(Some(1.0), 2),
            episode(Some(3.0), 3),
        ]);
        assert_eq!(e.progress(), 2);
    }

    #[test]
    fn update_progress_keeps_fractional_episode_numbers_distinct() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.update_progress(&[
            episode(Some(1.5), 1),
            episode(Some(1.5), 2),
            episode(Some(2.0), 3),
        ]);
        assert_eq!(e.progress(), 2);
    }

    #[test]
    fn update_progress_single_number_is_one() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.update_progress(&[episode(Some(4.0), 1)]);
        assert_eq!(e.progress(), 1);
    }

    #[test]
    fn update_progress_reaching_eps_completes_entity() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 2);
        e.update_progress(&[episode(Some(1.0), 1), episode(Some(2.0), 2)]);
        assert_eq!(e.sub_status(), SubAnimeStatus::Completed);
        assert!(e.is_completed());
    }

    #[test]
    fn update_progress_below_eps_stays_enable() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 3);
        e.update_progress(&[episode(Some(1.0), 1), episode(Some(2.0), 2)]);
        assert_eq!(e.sub_status(), SubAnimeStatus::Enable);
        assert!(!e.is_completed());
    }

    #[test]
    fn cancel_search_from_not_search_is_false() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        assert!(!e.cancel_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn cancel_search_from_matching_resets_status() {
        let mut e = entity(SubAnimeSearchStatus::Matching, 0, 12);
        assert!(e.cancel_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn cancel_search_from_searching_resets_status() {
        let mut e = entity(SubAnimeSearchStatus::Searching, 0, 12);
        assert!(e.cancel_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::NotSearch);
    }

    #[test]
    fn auto_bind_rule_binds_when_absent() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        assert_eq!(e.get_rule_id(), None);
        e.auto_bind_rule(5).expect("bind rule on empty slot");
        assert_eq!(e.get_rule_id(), Some(5));
    }

    #[test]
    fn auto_bind_rule_is_idempotent_for_same_rule() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.auto_bind_rule(5).expect("first bind");
        e.auto_bind_rule(5).expect("same rule binds again");
        assert_eq!(e.get_rule_id(), Some(5));
    }

    #[test]
    fn auto_bind_rule_rejects_different_rule() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.auto_bind_rule(5).expect("first bind");
        let err = e.auto_bind_rule(6).expect_err("different rule conflicts");
        assert!(err.to_string().contains("alreay binded"));
        assert_eq!(e.get_rule_id(), Some(5));
    }

    #[test]
    fn keywords_are_nfkc_normalised_and_lowercased() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.extend.titles = vec!["ＡＢＣ １２３".to_string(), "某番".to_string()];

        // 全角转半角、去空白、转小写；中文标题原样保留
        assert_eq!(e.keywords(), vec!["abc123".to_string(), "某番".to_string()]);
    }

    #[test]
    fn search_keywords_split_on_punctuation_then_sort_and_dedup() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.extend.titles = vec!["某番 ABC-第01集".to_string(), "某番".to_string()];

        // '-' 变成分隔符；重复的「某番」被 dedup 掉，结果按字节序排序
        assert_eq!(
            e.search_keywords(),
            vec!["ABC".to_string(), "某番".to_string(), "第01集".to_string()]
        );
    }

    struct RecordingUrlProvider {
        seen_keywords: std::sync::Mutex<Vec<Vec<String>>>,
    }

    impl FeedSearchUrlProvider for RecordingUrlProvider {
        fn made_search_url(&self, keywords: &[String]) -> Vec<SearchUrls> {
            self.seen_keywords
                .lock()
                .expect("keywords lock poisoned")
                .push(keywords.to_vec());
            vec![SearchUrls {
                feed_id: 7,
                urls: vec!["http://feed/7?q=某番".to_string()],
            }]
        }
    }

    #[test]
    fn get_search_urls_passes_deduped_keywords_to_provider() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        e.extend.titles = vec!["某番".to_string(), "某番".to_string()];
        let provider = RecordingUrlProvider {
            seen_keywords: std::sync::Mutex::new(Vec::new()),
        };

        let urls = e.get_search_urls(&provider);

        // 生产代码把自己算好的关键词原样交给 provider，重复标题只留一份
        assert_eq!(
            provider
                .seen_keywords
                .lock()
                .expect("keywords lock poisoned")
                .as_slice(),
            &[vec!["某番".to_string()]]
        );
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].feed_id, 7);
        assert_eq!(urls[0].urls, vec!["http://feed/7?q=某番".to_string()]);
    }

    #[test]
    fn enable_search_only_moves_not_search_to_pending() {
        let mut e = entity(SubAnimeSearchStatus::NotSearch, 0, 12);
        assert!(e.enable_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::Pending);
        // 已经进入搜索流程后不再重复唤醒
        assert!(!e.enable_search());
        assert_eq!(e.search_status(), SubAnimeSearchStatus::Pending);

        let mut searching = entity(SubAnimeSearchStatus::Searching, 0, 12);
        assert!(!searching.enable_search());
        assert_eq!(searching.search_status(), SubAnimeSearchStatus::Searching);
    }

    #[test]
    fn accessors_expose_identity_progress_and_rule_name() {
        let mut e = entity(SubAnimeSearchStatus::Searching, 4, 12);
        assert_eq!(e.id(), 1);
        assert_eq!(e.anime_id(), 100);
        assert_eq!(e.space_id(), 9);
        assert_eq!(e.progress(), 4);
        assert_eq!(e.eps_number(), 12);
        assert_eq!(e.sub_status(), SubAnimeStatus::Enable);
        assert!(!e.is_completed());
        assert_eq!(e.get_binding_rule_name(), None);
        assert_eq!(e.get_base_data().anime_id, 100);

        e.extend.rule_name = Some("规则X".to_string());
        e.data.progress = 12;
        assert_eq!(e.get_binding_rule_name(), Some("规则X"));
        assert_eq!(e.sub_status(), SubAnimeStatus::Completed);
        assert!(e.is_completed());
    }
}
