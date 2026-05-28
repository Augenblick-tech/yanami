use dashmap::DashMap;
use domain::{rule::RegexProvider, shared::error::DomainError};
use regex::Regex;

/// 基于模式字符串的正则表达式编译端口实现。
/// 内建缓存，避免相同模式重复编译。
/// 使用 DashMap 实现无锁并发读，写只锁单个分片。
pub struct CachingRegexProvider {
    cache: DashMap<String, Regex>,
}

impl Default for CachingRegexProvider {
    fn default() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }
}

impl RegexProvider for CachingRegexProvider {
    fn is_match(&self, pattern: &str, text: &str) -> Result<bool, DomainError> {
        if let Some(entry) = self.cache.get(pattern) {
            return Ok(entry.is_match(text));
        }
        let regex = Regex::new(pattern)
            .map_err(|_| DomainError::InvariantViolation("matching rule pattern is invalid"))?;
        let matched = regex.is_match(text);
        self.cache.insert(pattern.to_string(), regex);
        Ok(matched)
    }

    fn validate_and_cache(&self, pattern: &str) -> Result<(), DomainError> {
        if self.cache.contains_key(pattern) {
            return Ok(());
        }
        let regex = Regex::new(pattern)
            .map_err(|_| DomainError::InvariantViolation("matching rule pattern is invalid"))?;
        self.cache.insert(pattern.to_string(), regex);
        Ok(())
    }

    fn evict_pattern(&self, pattern: &str) {
        self.cache.remove(pattern);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use domain::{
        rule::{MatchingRule, SpaceRuleRepository},
        shared::error::DomainError,
        space::SpaceId,
    };

    use super::*;

    #[derive(Default)]
    struct RuleState {
        space_rules: HashMap<SpaceId, Vec<MatchingRule>>,
        space_loads: Vec<SpaceId>,
    }

    struct StubSpaceRules {
        state: Arc<Mutex<RuleState>>,
    }

    #[async_trait]
    impl SpaceRuleRepository for StubSpaceRules {
        async fn find_active_space_rules(
            &self,
            space_id: SpaceId,
        ) -> Result<Vec<MatchingRule>, DomainError> {
            let mut state = self.state.lock().expect("state");
            state.space_loads.push(space_id);
            Ok(state
                .space_rules
                .get(&space_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn find_space_rule(
            &self,
            space_id: SpaceId,
            rule_id: &domain::rule::MatchingRuleId,
        ) -> Result<Option<MatchingRule>, DomainError> {
            let state = self.state.lock().expect("state");
            Ok(state
                .space_rules
                .get(&space_id)
                .and_then(|rules| rules.iter().find(|rule| &rule.id == rule_id).cloned()))
        }

        async fn find_space_rule_by_name(
            &self,
            space_id: SpaceId,
            name: &str,
        ) -> Result<Option<MatchingRule>, DomainError> {
            let state = self.state.lock().expect("state");
            Ok(state
                .space_rules
                .get(&space_id)
                .and_then(|rules| rules.iter().find(|rule| rule.name == name).cloned()))
        }

        async fn save_space_rule(
            &self,
            _space_id: SpaceId,
            _rule: &MatchingRule,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn rule(name: &str, pattern: &str, order: u32) -> MatchingRule {
        MatchingRule {
            id: domain::rule::MatchingRuleId(format!("{name}_id")),
            name: name.to_string(),
            order,
            pattern: pattern.to_string(),
            active: true,
        }
    }
}
