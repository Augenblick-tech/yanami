use std::sync::Arc;

use domain::{
    rule::{MatchingRule, MatchingRuleId, RegexProvider, SpaceRuleRepository},
    shared::biz::BizContext,
    shared::error::DomainError,
    space::SpaceId,
};

use crate::entity::{validate_rule_list, RuleEntity};

#[derive(Clone)]
pub struct Rules {
    space_repository: Arc<dyn SpaceRuleRepository>,
    regex_provider: Arc<dyn RegexProvider>,
}

impl Rules {
    pub fn new(
        space_repository: Arc<dyn SpaceRuleRepository>,
        regex_provider: Arc<dyn RegexProvider>,
    ) -> Self {
        Self {
            space_repository,
            regex_provider,
        }
    }

    pub fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            space_repository: self.space_repository.with_biz(biz)?,
            regex_provider: self.regex_provider.clone(),
        })
    }

    pub fn regex_provider(&self) -> &dyn RegexProvider {
        self.regex_provider.as_ref()
    }

    pub fn create(&self, rule: MatchingRule) -> Result<RuleEntity, DomainError> {
        RuleEntity::new(rule, self.regex_provider.as_ref())
    }

    pub async fn list(&self, space_id: SpaceId) -> Result<Vec<RuleEntity>, DomainError> {
        let mut rules = self
            .space_repository
            .find_active_space_rules(space_id)
            .await?;
        rules.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then(left.id.0.cmp(&right.id.0))
        });
        rules
            .into_iter()
            .map(|r| RuleEntity::new(r, self.regex_provider.as_ref()))
            .collect()
    }

    pub async fn find_by_name_including_inactive(
        &self,
        space_id: SpaceId,
        name: &str,
    ) -> Result<Option<RuleEntity>, DomainError> {
        self.space_repository
            .find_space_rule_by_name(space_id, name)
            .await?
            .map(|r| RuleEntity::new(r, self.regex_provider.as_ref()))
            .transpose()
    }

    pub async fn save_rule(
        &self,
        space_id: SpaceId,
        mut entity: RuleEntity,
    ) -> Result<RuleEntity, DomainError> {
        entity.set_active();
        if let Some(existing) = self
            .space_repository
            .find_space_rule_by_name(space_id, &entity.read_data().name)
            .await?
        {
            entity.merge_or_reject(&existing)?;
        }
        let rule = entity.into_snapshot();

        let mut active_rules = self
            .space_repository
            .find_active_space_rules(space_id)
            .await?;
        if let Some(index) = active_rules
            .iter()
            .position(|existing| existing.id == rule.id)
        {
            active_rules[index] = rule.clone();
        } else {
            active_rules.push(rule.clone());
        }
        validate_rule_list(&active_rules, self.regex_provider.as_ref())?;
        self.space_repository
            .save_space_rule(space_id, &rule)
            .await?;
        RuleEntity::new(rule, self.regex_provider.as_ref())
    }

    pub async fn deactivate_rule(
        &self,
        space_id: SpaceId,
        rule_id: &MatchingRuleId,
    ) -> Result<RuleEntity, DomainError> {
        if rule_id.0.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "matching rule id cannot be empty",
            ));
        }
        let mut entity = self
            .space_repository
            .find_space_rule(space_id, rule_id)
            .await?
            .map(|r| RuleEntity::new(r, self.regex_provider.as_ref()))
            .transpose()?
            .ok_or(DomainError::InvariantViolation("matching rule not found"))?;
        let pattern = entity.read_data().pattern.clone();
        entity.deactivate();
        let rule = entity.into_snapshot();
        self.space_repository
            .save_space_rule(space_id, &rule)
            .await?;
        self.regex_provider.evict_pattern(&pattern);
        RuleEntity::new(rule, self.regex_provider.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;

    use super::*;

    struct RecordingRegexProvider {
        evicted: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingRegexProvider {
        fn new() -> Self {
            Self {
                evicted: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn evictions(&self) -> Vec<String> {
            self.evicted.lock().unwrap().clone()
        }
    }

    impl RegexProvider for RecordingRegexProvider {
        fn is_match(&self, _pattern: &str, _text: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        fn validate_and_cache(&self, _pattern: &str) -> Result<(), DomainError> {
            Ok(())
        }

        fn evict_pattern(&self, pattern: &str) {
            self.evicted.lock().unwrap().push(pattern.to_string());
        }
    }

    #[derive(Default)]
    struct InMemoryRules {
        rules: Mutex<HashMap<SpaceId, Vec<MatchingRule>>>,
    }

    #[async_trait]
    impl SpaceRuleRepository for InMemoryRules {
        async fn find_active_space_rules(
            &self,
            space_id: SpaceId,
        ) -> Result<Vec<MatchingRule>, DomainError> {
            Ok(self
                .rules
                .lock()
                .expect("rules")
                .get(&space_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|rule| rule.active)
                .collect())
        }

        async fn find_space_rule(
            &self,
            space_id: SpaceId,
            rule_id: &MatchingRuleId,
        ) -> Result<Option<MatchingRule>, DomainError> {
            Ok(self
                .rules
                .lock()
                .expect("rules")
                .get(&space_id)
                .and_then(|rules| rules.iter().find(|rule| &rule.id == rule_id).cloned()))
        }

        async fn find_space_rule_by_name(
            &self,
            space_id: SpaceId,
            name: &str,
        ) -> Result<Option<MatchingRule>, DomainError> {
            Ok(self
                .rules
                .lock()
                .expect("rules")
                .get(&space_id)
                .and_then(|rules| rules.iter().find(|rule| rule.name == name).cloned()))
        }

        async fn save_space_rule(
            &self,
            space_id: SpaceId,
            rule: &MatchingRule,
        ) -> Result<(), DomainError> {
            let mut rules = self.rules.lock().expect("rules");
            let list = rules.entry(space_id).or_default();
            if let Some(index) = list.iter().position(|existing| existing.id == rule.id) {
                list[index] = rule.clone();
            } else {
                list.push(rule.clone());
            }
            Ok(())
        }
    }

    struct AlwaysFalseRegexProvider;

    impl RegexProvider for AlwaysFalseRegexProvider {
        fn is_match(&self, _pattern: &str, _text: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        fn validate_and_cache(&self, _pattern: &str) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn rule(id: &str, name: &str, order: u32, active: bool) -> MatchingRule {
        MatchingRule {
            id: MatchingRuleId(id.to_string()),
            name: name.to_string(),
            order,
            pattern: format!("^{name}"),
            active,
        }
    }

    #[tokio::test]
    async fn list_returns_only_active_rules() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![
                rule("active", "ANi", 1, true),
                rule("deleted", "Lilith", 2, false),
            ],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let listed = rules.list(SpaceId(1)).await.expect("list");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].read_data().id.0, "active");
    }

    #[tokio::test]
    async fn deactivated_rule_remains_addressable_by_name() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![
                rule("active", "ANi", 1, true),
                rule("to-delete", "Lilith", 2, true),
            ],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let deleted = rules
            .deactivate_rule(SpaceId(1), &MatchingRuleId("to-delete".to_string()))
            .await
            .expect("deactivate");
        let listed = rules.list(SpaceId(1)).await.expect("list active");
        let by_name = rules
            .find_by_name_including_inactive(SpaceId(1), "Lilith")
            .await
            .expect("find by name")
            .expect("deleted rule");

        assert!(!deleted.read_data().active);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].read_data().id.0, "active");
        assert_eq!(by_name.read_data().id.0, "to-delete");
        assert!(!by_name.read_data().active);
    }

    #[tokio::test]
    async fn save_rule_rejects_active_rule_with_same_name_and_different_id() {
        let repository = Arc::new(InMemoryRules::default());
        repository
            .rules
            .lock()
            .expect("rules")
            .insert(SpaceId(1), vec![rule("ani", "ANi", 1, true)]);
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let new_rule = MatchingRule {
            id: MatchingRuleId("ani-v2".to_string()),
            name: "ANi".to_string(),
            order: 2,
            pattern: r"^\[ANi-V2\].*$".to_string(),
            active: true,
        };

        let error = rules
            .save_rule(
                SpaceId(1),
                RuleEntity::new(new_rule, &AlwaysFalseRegexProvider).expect("entity"),
            )
            .await
            .expect_err("name conflict");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule name must be unique"
        );
    }

    #[tokio::test]
    async fn save_rule_merges_into_inactive_rule_with_same_name() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![
                rule("ani", "ANi", 1, true),
                rule("lilith", "Lilith", 2, false),
            ],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let new_rule = MatchingRule {
            id: MatchingRuleId("new-lilith".to_string()),
            name: "Lilith".to_string(),
            order: 3,
            pattern: r"^\[Lilith-V2\].*$".to_string(),
            active: true,
        };

        let saved = rules
            .save_rule(
                SpaceId(1),
                RuleEntity::new(new_rule, &AlwaysFalseRegexProvider).expect("entity"),
            )
            .await
            .expect("save merged");

        assert_eq!(saved.read_data().id.0, "lilith");
        assert_eq!(saved.read_data().name, "Lilith");
        assert_eq!(saved.read_data().order, 3);
        assert_eq!(saved.read_data().pattern, r"^\[Lilith-V2\].*$");
        assert!(saved.read_data().active);

        let listed = rules.list(SpaceId(1)).await.expect("list");
        assert_eq!(listed.len(), 2);
    }

    #[tokio::test]
    async fn save_rule_creates_new_rule_without_name_conflict() {
        let repository = Arc::new(InMemoryRules::default());
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let new_rule = MatchingRule {
            id: MatchingRuleId("new-rule".to_string()),
            name: "MyGroup".to_string(),
            order: 1,
            pattern: r"^\[MyGroup\].*$".to_string(),
            active: true,
        };

        let saved = rules
            .save_rule(
                SpaceId(1),
                RuleEntity::new(new_rule, &AlwaysFalseRegexProvider).expect("entity"),
            )
            .await
            .expect("save new rule");

        assert_eq!(saved.read_data().id.0, "new-rule");
        assert_eq!(saved.read_data().name, "MyGroup");
        assert_eq!(saved.read_data().order, 1);
        assert!(saved.read_data().active);

        let listed = rules.list(SpaceId(1)).await.expect("list");
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn deactivate_rule_rejects_empty_id() {
        let repository = Arc::new(InMemoryRules::default());
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let error = rules
            .deactivate_rule(SpaceId(1), &MatchingRuleId(String::new()))
            .await
            .expect_err("empty id");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule id cannot be empty"
        );
    }

    #[tokio::test]
    async fn deactivate_rule_rejects_not_found() {
        let repository = Arc::new(InMemoryRules::default());
        let rules = Rules::new(
            repository,
            Arc::new(AlwaysFalseRegexProvider),
        );

        let error = rules
            .deactivate_rule(SpaceId(1), &MatchingRuleId("nonexistent".to_string()))
            .await
            .expect_err("not found");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule not found"
        );
    }

    #[tokio::test]
    async fn deactivate_rule_calls_evict_pattern() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![rule("to-evict", "MyGroup", 1, true)],
        );
        let regex_provider = Arc::new(RecordingRegexProvider::new());
        let rules = Rules::new(repository, regex_provider.clone());

        rules
            .deactivate_rule(SpaceId(1), &MatchingRuleId("to-evict".to_string()))
            .await
            .expect("deactivate");

        let evictions = regex_provider.evictions();
        assert_eq!(evictions.len(), 1);
        assert_eq!(evictions[0], "^MyGroup");
    }

    #[tokio::test]
    async fn save_rule_updates_existing_rule() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![rule("rule1", "Original", 1, true)],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let updated_rule = MatchingRule {
            id: MatchingRuleId("rule1".to_string()),
            name: "Original".to_string(),
            order: 2,
            pattern: r"^\[Updated\].*$".to_string(),
            active: true,
        };

        let saved = rules
            .save_rule(
                SpaceId(1),
                RuleEntity::new(updated_rule, &AlwaysFalseRegexProvider).expect("entity"),
            )
            .await
            .expect("update rule");

        assert_eq!(saved.read_data().order, 2);
        assert_eq!(saved.read_data().pattern, r"^\[Updated\].*$");
        assert!(saved.read_data().active);

        let listed = rules.list(SpaceId(1)).await.expect("list");
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn list_returns_sorted_rules() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![
                rule("z", "Last", 2, true),
                rule("a", "First", 1, true),
                MatchingRule {
                    id: MatchingRuleId("b".to_string()),
                    name: "Middle".to_string(),
                    order: 1,
                    pattern: "^Middle$".to_string(),
                    active: true,
                },
            ],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let listed = rules.list(SpaceId(1)).await.expect("list");

        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].read_data().id.0, "a");
        assert_eq!(listed[1].read_data().id.0, "b");
        assert_eq!(listed[2].read_data().id.0, "z");
    }

    #[tokio::test]
    async fn find_by_name_including_inactive_returns_none_for_missing() {
        let repository = Arc::new(InMemoryRules::default());
        repository.rules.lock().expect("rules").insert(
            SpaceId(1),
            vec![rule("existing", "Present", 1, true)],
        );
        let rules = Rules::new(repository, Arc::new(AlwaysFalseRegexProvider));

        let result = rules
            .find_by_name_including_inactive(SpaceId(1), "NonExistent")
            .await
            .expect("find by name");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn regex_provider_returns_provider() {
        let provider = Arc::new(AlwaysFalseRegexProvider);
        let repository = Arc::new(InMemoryRules::default());
        let rules = Rules::new(repository, provider.clone());

        let retrieved = rules.regex_provider();

        let result = retrieved.is_match("test", "text");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
