use std::sync::Arc;

use async_trait::async_trait;

use crate::entity::{
    cap::{RuleMatcher, SpaceRuleMatcher},
    model::RuleBaseData,
};

#[derive(Clone)]
pub struct SpaceRules {
    data: Vec<RuleBaseData>,
    matcher: Arc<dyn RuleMatcher>,
}

impl SpaceRules {
    pub(super) fn new(mut data: Vec<RuleBaseData>, matcher: Arc<dyn RuleMatcher>) -> Self {
        data.sort_by_key(|x| x.metadata.order);
        Self { data, matcher }
    }
}

#[async_trait]
impl SpaceRuleMatcher for SpaceRules {
    fn is_match(&self, text: &str) -> (bool, i64) {
        for i in &self.data {
            if self.matcher.is_match(&i.metadata.pattern, text) {
                return (true, i.id);
            }
        }
        (false, 0)
    }
}
