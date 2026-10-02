use async_trait::async_trait;

use crate::entity::{
    cap::{RuleMatcher, SpaceRuleMatcher},
    model::{MatchResult, RuleBaseData},
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
    fn is_match(&self, text: &str) -> MatchResult {
        MatchResult {
            matched: self.is_match(text),
            rule_id: self.id(),
            rule_order: self.order(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dashmap::DashMap;
    use regex::Regex;

    use super::RuleEntity;
    use crate::entity::cap::{RuleMatcher, SpaceRuleMatcher};
    use crate::entity::model::{Rule, RuleBaseData};
    use crate::infra::regex::RegexRuleMatcher;

    fn entity(id: i64, order: i64, pattern: &str, active: bool) -> RuleEntity {
        let matcher: Arc<dyn RuleMatcher> =
            Arc::new(RegexRuleMatcher::new(Arc::new(DashMap::<String, Regex>::new())));
        RuleEntity::new(
            RuleBaseData {
                id,
                active,
                metadata: Rule {
                    space_id: 3,
                    name: "海贼".to_string(),
                    order,
                    pattern: pattern.to_string(),
                },
            },
            matcher,
        )
    }

    #[test]
    fn accessors_expose_rule_metadata() {
        let e = entity(7, 4, "^某番", true);
        assert_eq!(e.id(), 7);
        assert_eq!(e.space_id(), 3);
        assert_eq!(e.name(), "海贼");
        assert_eq!(e.order(), 4);
        assert_eq!(e.pattern(), "^某番");
        assert!(e.active());
    }

    #[test]
    fn is_match_uses_real_regex_matcher() {
        let e = entity(7, 4, "^某番", true);
        assert!(e.is_match("某番 第01集"));
        assert!(!e.is_match("别的番 第01集"));
    }

    #[test]
    fn set_order_updates_order() {
        let mut e = entity(7, 5, "^某番", true);
        e.set_order(2);
        assert_eq!(e.order(), 2);
    }

    #[test]
    fn space_rule_matcher_reports_rule_identity_on_match() {
        let e = entity(7, 4, "^某番", true);
        let res = SpaceRuleMatcher::is_match(&e, "某番 第01集");
        assert!(res.matched);
        assert_eq!(res.rule_id, 7);
        assert_eq!(res.rule_order, 4);
    }

    #[test]
    fn space_rule_matcher_no_match_keeps_rule_identity() {
        let e = entity(7, 4, "^别", true);
        let res = SpaceRuleMatcher::is_match(&e, "某番 第01集");
        assert!(!res.matched);
        // 未命中时仍回填规则身份，由调用方按 matched 判断
        assert_eq!(res.rule_id, 7);
        assert_eq!(res.rule_order, 4);
    }
}
