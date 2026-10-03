use std::sync::Arc;

use async_trait::async_trait;

use crate::entity::{
    cap::{RuleMatcher, SpaceRuleMatcher},
    model::{MatchResult, RuleBaseData},
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
    fn is_match(&self, text: &str) -> MatchResult {
        for i in &self.data {
            if self.matcher.is_match(&i.metadata.pattern, text) {
                return MatchResult {
                    matched: true,
                    rule_id: i.id,
                    rule_order: i.metadata.order,
                };
            }
        }
        MatchResult {
            matched: false,
            rule_id: 0,
            rule_order: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dashmap::DashMap;
    use regex::Regex;

    use super::SpaceRules;
    use crate::entity::cap::{RuleMatcher, SpaceRuleMatcher};
    use crate::entity::model::{Rule, RuleBaseData};
    use crate::infra::regex::RegexRuleMatcher;

    fn matcher() -> Arc<dyn RuleMatcher> {
        Arc::new(RegexRuleMatcher::new(Arc::new(
            DashMap::<String, Regex>::new(),
        )))
    }

    fn rule(id: i64, order: i64, pattern: &str) -> RuleBaseData {
        RuleBaseData {
            id,
            active: true,
            metadata: Rule {
                space_id: 1,
                name: format!("规则-{id}"),
                order,
                pattern: pattern.to_string(),
            },
        }
    }

    #[test]
    fn is_match_returns_lowest_order_rule_when_both_match() {
        // 传入顺序被打乱，new 会按 order 排序，两条都命中时取 order 更小的一条
        let rules = SpaceRules::new(vec![rule(20, 2, "第0?1集"), rule(10, 1, "某番")], matcher());
        let res = SpaceRuleMatcher::is_match(&rules, "某番 第01集");
        assert!(res.matched);
        assert_eq!(res.rule_id, 10);
        assert_eq!(res.rule_order, 1);
    }

    #[test]
    fn is_match_falls_through_to_next_rule_when_first_misses() {
        let rules = SpaceRules::new(
            vec![rule(20, 2, "第0?1集"), rule(10, 1, "别的番")],
            matcher(),
        );
        let res = SpaceRuleMatcher::is_match(&rules, "某番 第01集");
        assert!(res.matched);
        assert_eq!(res.rule_id, 20);
        assert_eq!(res.rule_order, 2);
    }

    #[test]
    fn is_match_returns_zero_when_no_rule_matches() {
        let rules = SpaceRules::new(vec![rule(10, 1, "不存在的番")], matcher());
        let res = SpaceRuleMatcher::is_match(&rules, "某番 第01集");
        assert!(!res.matched);
        assert_eq!(res.rule_id, 0);
        assert_eq!(res.rule_order, 0);
    }

    #[test]
    fn is_match_with_empty_rule_set_is_no_match() {
        let rules = SpaceRules::new(Vec::new(), matcher());
        let res = SpaceRuleMatcher::is_match(&rules, "某番 第01集");
        assert!(!res.matched);
        assert_eq!(res.rule_id, 0);
        assert_eq!(res.rule_order, 0);
    }
}
