use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use domain::{
    rule::{MatchingRule, RegexProvider, SpaceRuleRepository},
    shared::error::DomainError,
    space::SpaceId,
};
use regex::Regex;
use rule::entity::RuleEntity;
use subscription::shared::error::ApplicationError;

pub struct CachingRuleRuntime {
    space_rules: Arc<dyn SpaceRuleRepository>,
    cache: RwLock<HashMap<SpaceId, Arc<CompiledRules>>>,
}

impl CachingRuleRuntime {
    pub fn new(space_rules: Arc<dyn SpaceRuleRepository>) -> Self {
        Self {
            space_rules,
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn with_cache_read<R>(&self, f: impl FnOnce(&HashMap<SpaceId, Arc<CompiledRules>>) -> R) -> R {
        match self.cache.read() {
            Ok(cache) => f(&cache),
            Err(poisoned) => {
                let cache = poisoned.into_inner();
                f(&cache)
            }
        }
    }

    fn with_cache_write<R>(
        &self,
        f: impl FnOnce(&mut HashMap<SpaceId, Arc<CompiledRules>>) -> R,
    ) -> R {
        match self.cache.write() {
            Ok(mut cache) => f(&mut cache),
            Err(poisoned) => {
                let mut cache = poisoned.into_inner();
                cache.clear();
                f(&mut cache)
            }
        }
    }

    async fn load_space_compiled(
        &self,
        space_id: SpaceId,
    ) -> Result<Arc<CompiledRules>, ApplicationError> {
        if let Some(compiled) = self.with_cache_read(|cache| cache.get(&space_id).cloned()) {
            return Ok(compiled);
        }

        let rules = self.space_rules.find_active_space_rules(space_id).await?;
        let compiled = Arc::new(compile_rules(rules)?);
        self.with_cache_write(|cache| {
            cache.insert(space_id, compiled.clone());
        });
        Ok(compiled)
    }

    pub fn invalidate_space_rules(&self, space_id: SpaceId) {
        self.with_cache_write(|cache| {
            cache.remove(&space_id);
        });
    }
}

impl CachingRuleRuntime {
    pub async fn match_space_rule(
        &self,
        space_id: SpaceId,
        title: &str,
    ) -> Result<Option<MatchingRule>, ApplicationError> {
        let compiled = self.load_space_compiled(space_id).await?;
        for compiled_rule in &compiled.rules {
            if compiled_rule.regex.is_match(title) {
                return Ok(Some(compiled_rule.rule.clone()));
            }
        }
        Ok(None)
    }
}

struct CompiledRules {
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    rule: MatchingRule,
    regex: Regex,
}

fn compile_rules(rules: Vec<MatchingRule>) -> Result<CompiledRules, ApplicationError> {
    let mut compiled_rules = Vec::with_capacity(rules.len());
    for rule in rules {
        let entity = RuleEntity::new(rule)?;
        let rule = entity.read_data();
        compiled_rules.push(CompiledRule {
            rule: rule.clone(),
            regex: Regex::new(&rule.pattern).map_err(|_| {
                domain::shared::error::DomainError::InvariantViolation(
                    "matching rule pattern is invalid",
                )
            })?,
        });
    }
    Ok(CompiledRules {
        rules: compiled_rules,
    })
}

/// 基于模式字符串的正则表达式编译端口实现。
/// 内建缓存，避免相同模式重复编译。
pub struct CachingRegexProvider {
    cache: RwLock<HashMap<String, Arc<Regex>>>,
}

impl Default for CachingRegexProvider {
    fn default() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }
}

impl RegexProvider for CachingRegexProvider {
    fn is_match(&self, pattern: &str, text: &str) -> Result<bool, DomainError> {
        if let Ok(cache) = self.cache.read() {
            if let Some(regex) = cache.get(pattern) {
                return Ok(regex.is_match(text));
            }
        }
        let regex = Regex::new(pattern)
            .map_err(|_| DomainError::InvariantViolation("matching rule pattern is invalid"))?;
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(pattern.to_string(), Arc::new(regex));
            if let Some(regex) = cache.get(pattern) {
                return Ok(regex.is_match(text));
            }
        }
        Err(DomainError::InvariantViolation("regex cache lock poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::shared::error::DomainError;

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

    #[tokio::test]
    async fn caches_space_rules_and_invalidates_them() {
        let state = Arc::new(Mutex::new(RuleState::default()));
        state
            .lock()
            .expect("state")
            .space_rules
            .insert(SpaceId(1), vec![rule("ani", "ANi", 1)]);
        let runtime = CachingRuleRuntime::new(Arc::new(StubSpaceRules {
            state: Arc::clone(&state),
        }));

        let first = runtime
            .match_space_rule(SpaceId(1), "[ANi] Hyakkiyakosho")
            .await
            .expect("first");
        let second = runtime
            .match_space_rule(SpaceId(1), "[ANi] Hyakkiyakosho")
            .await
            .expect("second");
        runtime.invalidate_space_rules(SpaceId(1));
        let third = runtime
            .match_space_rule(SpaceId(1), "[ANi] Hyakkiyakosho")
            .await
            .expect("third");

        let state = state.lock().expect("state");
        assert_eq!(first.expect("rule").name, "ani");
        assert_eq!(second.expect("rule").name, "ani");
        assert_eq!(third.expect("rule").name, "ani");
        assert_eq!(state.space_loads.len(), 2);
    }

    #[tokio::test]
    async fn rejects_invalid_regex_patterns() {
        let state = Arc::new(Mutex::new(RuleState::default()));
        state
            .lock()
            .expect("state")
            .space_rules
            .insert(SpaceId(1), vec![rule("bad", "(", 1)]);
        let runtime = CachingRuleRuntime::new(Arc::new(StubSpaceRules { state }));

        let error = runtime
            .match_space_rule(SpaceId(1), "anything")
            .await
            .expect_err("invalid regex");

        assert!(error
            .to_string()
            .contains("matching rule pattern is invalid"));
    }
}
