use async_trait::async_trait;

use crate::entity::{
    cap::{RuleMatcher, SpaceRuleMatcher},
    model::RuleBaseData,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct RuleEntity {
    data: RuleBaseData,
    matcher: Arc<dyn RuleMatcher>,
}

impl RuleEntity {
    pub(super) fn new(rule: RuleBaseData, matcher: Arc<dyn RuleMatcher>) -> Self {
        Self {
            data: rule,
            matcher,
        }
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn space_id(&self) -> i64 {
        self.data.metadata.space_id
    }

    pub fn name(&self) -> &str {
        &self.data.metadata.name
    }

    pub fn order(&self) -> i64 {
        self.data.metadata.order
    }

    pub fn pattern(&self) -> &str {
        &self.data.metadata.pattern
    }

    pub fn active(&self) -> bool {
        self.data.active
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.matcher.is_match(&self.data.metadata.pattern, text)
    }

    pub fn set_order(&mut self, order: i64) {
        self.data.metadata.order = order;
    }
}

impl RuleEntity {
    pub(super) fn get_base_data(&self) -> &RuleBaseData {
        &self.data
    }
}

#[async_trait]
impl SpaceRuleMatcher for RuleEntity {
    fn is_match(&self, text: &str) -> (bool, i64) {
        (self.is_match(text), self.id())
    }
}
