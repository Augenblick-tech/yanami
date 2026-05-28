use domain::{
    rule::{MatchingRule, RegexProvider},
    shared::error::DomainError,
};

#[path = "match_rule.rs"]
mod match_rule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEntity {
    rule: MatchingRule,
}

impl RuleEntity {
    pub fn new(rule: MatchingRule, p: &dyn RegexProvider) -> Result<Self, DomainError> {
        validate_rule(&rule, p)?;
        Ok(Self { rule })
    }

    pub fn read_data(&self) -> &MatchingRule {
        &self.rule
    }

    pub fn into_snapshot(self) -> MatchingRule {
        self.rule
    }

    pub fn set_active(&mut self) {
        self.rule.active = true;
    }

    pub fn merge_or_reject(&mut self, existing: &MatchingRule) -> Result<(), DomainError> {
        if existing.active && existing.id != self.rule.id {
            return Err(DomainError::InvariantViolation(
                "matching rule name must be unique",
            ));
        }
        if !existing.active && existing.id != self.rule.id {
            self.rule.id = existing.id.clone();
        }
        Ok(())
    }

    pub fn deactivate(&mut self) {
        self.rule.active = false;
    }
}

pub(crate) fn validate_rule_list(rules: &[MatchingRule], p: &dyn RegexProvider) -> Result<(), DomainError> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_active_orders = std::collections::HashSet::new();

    for rule in rules {
        validate_rule(rule, p)?;
        if !seen_ids.insert(rule.id.0.clone()) {
            return Err(DomainError::InvariantViolation(
                "matching rule id must be unique",
            ));
        }
        if rule.active && !seen_active_orders.insert(rule.order) {
            return Err(DomainError::InvariantViolation(
                "active matching rule order must be unique",
            ));
        }
    }

    Ok(())
}

fn validate_rule(rule: &MatchingRule, p: &dyn RegexProvider) -> Result<(), DomainError> {
    if rule.id.0.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "matching rule id cannot be empty",
        ));
    }
    if rule.name.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "matching rule name cannot be empty",
        ));
    }
    if rule.pattern.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "matching rule pattern cannot be empty",
        ));
    }
    p.validate_and_cache(&rule.pattern)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf};

    use domain::rule::{MatchingRuleId, RegexProvider};
    use regex::Regex;
    use rss::Channel;

    use super::*;

    struct TestRegexProvider;

    impl RegexProvider for TestRegexProvider {
        fn is_match(
            &self,
            pattern: &str,
            text: &str,
        ) -> Result<bool, domain::shared::error::DomainError> {
            let regex = Regex::new(pattern).map_err(|_| {
                domain::shared::error::DomainError::InvariantViolation("invalid regex")
            })?;
            Ok(regex.is_match(text))
        }

        fn validate_and_cache(
            &self,
            pattern: &str,
        ) -> Result<(), domain::shared::error::DomainError> {
            Regex::new(pattern).map_err(|_| {
                domain::shared::error::DomainError::InvariantViolation("matching rule pattern is invalid")
            })?;
            Ok(())
        }
    }

    fn sample_rule(id: &str, name: &str, order: u32, pattern: &str) -> MatchingRule {
        MatchingRule {
            id: MatchingRuleId(id.to_string()),
            name: name.to_string(),
            order,
            pattern: pattern.to_string(),
            active: true,
        }
    }

    fn snapshot_title(prefix: &str) -> String {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../infra/tests/fixtures/dmhy_search_hyakkiyakosho.xml");
        let content = std::fs::read(fixture).expect("read dmhy snapshot");
        let channel = Channel::read_from(Cursor::new(content)).expect("parse dmhy rss");
        channel
            .items()
            .iter()
            .filter_map(|item| item.title())
            .find(|title| title.contains(prefix))
            .expect("snapshot title")
            .to_string()
    }

    #[test]
    fn new_validates_rule() {
        let entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");

        assert_eq!(entity.read_data().id.0, "a");
        assert_eq!(entity.read_data().name, "ANi");
        assert_eq!(entity.read_data().order, 1);
        assert_eq!(entity.read_data().pattern, r"^\[ANi\].*$");
        assert!(entity.read_data().active);
    }

    #[test]
    fn new_rejects_invalid_regex() {
        let error =
            RuleEntity::new(sample_rule("a", "broken", 1, "["), &TestRegexProvider)
                .expect_err("invalid regex");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule pattern is invalid"
        );
    }

    #[test]
    fn rule_list_validation_rejects_duplicate_order() {
        let error = validate_rule_list(
            &[
                sample_rule("a", "ANi", 1, r"^\[ANi\].*$"),
                sample_rule("b", "Lilith", 1, r"^\[Lilith\].*$"),
            ],
            &TestRegexProvider,
        )
        .expect_err("duplicate order");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: active matching rule order must be unique"
        );
    }

    #[test]
    fn rule_list_validation_allows_inactive_duplicate_order() {
        let mut inactive = sample_rule("b", "Lilith", 1, r"^\[Lilith\].*$");
        inactive.active = false;

        let rules = &[sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), inactive];
        let result = validate_rule_list(rules, &TestRegexProvider);
        assert!(result.is_ok());
    }

    #[test]
    fn merge_or_reject_rejects_active_name_conflict() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");
        let existing = sample_rule("b", "ANi", 2, r"^\[ANi\].*$");

        let error = entity
            .merge_or_reject(&existing)
            .expect_err("active conflict");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule name must be unique"
        );
    }

    #[test]
    fn merge_or_reject_adopts_inactive_id() {
        let mut entity =
            RuleEntity::new(sample_rule("new-id", "Lilith", 3, r"^\[Lilith\].*$"), &TestRegexProvider)
                .expect("entity");
        let mut existing = sample_rule("old-id", "Lilith", 2, r"^\[Lilith\].*$");
        existing.active = false;

        entity.merge_or_reject(&existing).expect("adopt id");

        assert_eq!(entity.read_data().id.0, "old-id");
    }

    #[test]
    fn match_title_uses_rule_regex() {
        let entity =
            RuleEntity::new(sample_rule("ani", "ANi", 1, r"^\[ANi\].*"), &TestRegexProvider)
                .expect("entity");
        let title = snapshot_title("[ANi]");

        let matched = entity
            .match_title(&TestRegexProvider, &title)
            .expect("match")
            .expect("matched");

        assert_eq!(matched.id.0, "ani");
    }

    #[test]
    fn new_rejects_empty_id() {
        let error =
            RuleEntity::new(sample_rule("", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect_err("empty id");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule id cannot be empty"
        );
    }

    #[test]
    fn new_rejects_empty_name() {
        let error =
            RuleEntity::new(sample_rule("a", "", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect_err("empty name");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule name cannot be empty"
        );
    }

    #[test]
    fn new_rejects_empty_pattern() {
        let error =
            RuleEntity::new(sample_rule("a", "ANi", 1, ""), &TestRegexProvider)
                .expect_err("empty pattern");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule pattern cannot be empty"
        );
    }

    #[test]
    fn rule_list_validation_rejects_duplicate_id() {
        let error = validate_rule_list(
            &[
                sample_rule("a", "ANi", 1, r"^\[ANi\].*$"),
                sample_rule("a", "Lilith", 2, r"^\[Lilith\].*$"),
            ],
            &TestRegexProvider,
        )
        .expect_err("duplicate id");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule id must be unique"
        );
    }

    #[test]
    fn match_title_returns_none_when_not_matched() {
        let entity =
            RuleEntity::new(sample_rule("ani", "ANi", 1, r"^\[NoMatch\].*"), &TestRegexProvider)
                .expect("entity");
        let title = snapshot_title("[ANi]");

        let matched = entity
            .match_title(&TestRegexProvider, &title)
            .expect("match");

        assert!(matched.is_none());
    }

    #[test]
    fn deactivate_sets_active_false() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");

        entity.deactivate();

        assert!(!entity.read_data().active);
    }

    #[test]
    fn set_active_sets_active_true() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");
        entity.deactivate();

        entity.set_active();

        assert!(entity.read_data().active);
    }

    #[test]
    fn merge_or_reject_self_update_preserves_id() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");
        let existing = sample_rule("a", "ANi", 1, r"^\[ANi\].*$");

        entity.merge_or_reject(&existing).expect("self-update");

        assert_eq!(entity.read_data().id.0, "a");
    }

    #[test]
    fn deactivate_is_idempotent() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");

        entity.deactivate();
        assert!(!entity.read_data().active);

        entity.deactivate();
        assert!(!entity.read_data().active);
    }

    #[test]
    fn set_active_is_idempotent() {
        let mut entity =
            RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");
        entity.set_active();

        entity.set_active();
        assert!(entity.read_data().active);

        entity.set_active();
        assert!(entity.read_data().active);
    }

    #[test]
    fn new_rejects_whitespace_only_fields() {
        let error = RuleEntity::new(sample_rule("  ", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
            .expect_err("whitespace id");
        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule id cannot be empty"
        );

        let error = RuleEntity::new(sample_rule("a", "  ", 1, r"^\[ANi\].*$"), &TestRegexProvider)
            .expect_err("whitespace name");
        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule name cannot be empty"
        );

        let error =
            RuleEntity::new(sample_rule("a", "ANi", 1, "  "), &TestRegexProvider)
                .expect_err("whitespace pattern");
        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule pattern cannot be empty"
        );
    }

    #[test]
    fn rule_list_validation_accepts_empty_list() {
        let result = validate_rule_list(&[], &TestRegexProvider);
        assert!(result.is_ok());
    }

    #[test]
    fn rule_list_validation_accepts_valid_list() {
        let rules = &[
            sample_rule("a", "ANi", 1, r"^\[ANi\].*$"),
            sample_rule("b", "Lilith", 2, r"^\[Lilith\].*$"),
        ];
        let result = validate_rule_list(rules, &TestRegexProvider);
        assert!(result.is_ok());
    }

    #[test]
    fn into_snapshot_returns_inner_data() {
        let entity =
            RuleEntity::new(sample_rule("ani", "ANi", 1, r"^\[ANi\].*$"), &TestRegexProvider)
                .expect("entity");

        let snapshot = entity.into_snapshot();

        assert_eq!(snapshot.id.0, "ani");
        assert_eq!(snapshot.name, "ANi");
        assert_eq!(snapshot.order, 1);
        assert_eq!(snapshot.pattern, r"^\[ANi\].*$");
        assert!(snapshot.active);
    }
}
