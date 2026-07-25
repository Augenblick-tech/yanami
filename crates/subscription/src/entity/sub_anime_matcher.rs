use std::sync::Arc;

use chrono::NaiveDateTime;
use common::shared::error::Error;
use resource::entity::resource_entity::ResourceEntity;

use crate::entity::{
    cap::SpaceRuleMatcher,
    model::{EpsiodeStatus, MatchedEpisode as Epsiode},
};

#[derive(Clone)]
pub struct SubAnimeMatcher {
    id: i64,
    rule_id: Option<i64>,
    keywords: Vec<String>,
    eps: Vec<Epsiode>,
    eps_num: u32,
    time_range: std::ops::Range<i64>,

    matcher: Arc<dyn SpaceRuleMatcher>,
}

impl SubAnimeMatcher {
    pub(super) fn new(
        id: i64,
        rule_id: Option<i64>,
        eps_num: u32,
        keywords: Vec<String>,
        matcher: Arc<dyn SpaceRuleMatcher>,
        time_range: std::ops::Range<NaiveDateTime>,
    ) -> Self {
        let start = time_range.start.and_utc().timestamp();
        let end = time_range.end.and_utc().timestamp();
        Self {
            id,
            rule_id,
            eps_num,
            keywords,
            eps: vec![],
            time_range: start..end,
            matcher,
        }
    }

    pub(super) fn get_match_eps(&self) -> &[Epsiode] {
        &self.eps
    }

    pub(super) fn get_rule_id(&self) -> Option<i64> {
        self.rule_id
    }

    pub(super) fn eps_number(&self) -> u32 {
        self.eps_num
    }

    pub fn sub_anime_id(&self) -> i64 {
        self.id
    }

    pub fn match_resource(&mut self, res: &ResourceEntity) -> Result<bool, Error> {
        // 时间范围过滤
        if !self.time_range.contains(&res.published_at()) {
            return Ok(false);
        }

        // 判断是否已经匹配过
        if self
            .eps
            .iter()
            .find(|i| &i.resource_id == res.id())
            .is_some()
        {
            return Ok(true);
        }

        // 过滤规则
        let (matched, rule_id) = self.matcher.is_match(res.title());
        if !matched {
            return Ok(matched);
        }
        let mut title_match = false;

        // 匹配关键字
        for i in &self.keywords {
            if res.match_title().contains(i) {
                title_match = true;
                break;
            }
        }
        if !title_match {
            return Ok(title_match);
        }

        if let Some(id) = self.rule_id
            && rule_id != id
        {
            return Err(Error::conflict(format!(
                "sub anime matcher match resource failed, got matched rule id {}, but bind {}",
                rule_id, id
            )));
        }
        self.rule_id = Some(rule_id);
        self.eps.push(Epsiode {
            sub_anime_id: self.id,
            resource_id: *res.id(),
            title: res.title().into(),
            status: EpsiodeStatus::Pending,
        });

        Ok(matched)
    }
}
