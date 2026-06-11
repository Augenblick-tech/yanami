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
        let eps_numbers = eps.iter().filter_map(|i| i.ep_num).collect::<Vec<_>>();
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
        if self.data.progress >= self.extend.eps {
            SubAnimeStatus::Completed
        } else {
            SubAnimeStatus::Enable
        }
    }

    pub fn is_completed(&self) -> bool {
        self.data.progress >= self.extend.eps
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

    pub fn cancel_search(&mut self) {
        if self.data.search_status != SubAnimeSearchStatus::NotSearch {
            self.data.search_status = SubAnimeSearchStatus::NotSearch;
        }
    }

    pub fn get_search_urls(
        &self,
        search_url_provider: &dyn FeedSearchUrlProvider,
    ) -> Vec<SearchUrls> {
        let keywords = self.keywords();
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
    fn request_search(&mut self) -> bool {
        use SubAnimeSearchStatus::*;
        if self.is_completed() {
            self.cancel_search();
            return false;
        }

        if self.data.search_status == Matching || self.data.search_status == Searching {
            self.data.search_status = Searching;
            return true;
        }
        false
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
