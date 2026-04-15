use domain::{
    rule::{capability::RuleWriterCap, MatchingRule},
    shared::error::DomainError,
    space::SpaceId,
};
use regex::Regex;

#[path = "match_rule.rs"]
mod match_rule;
#[path = "validate.rs"]
mod validate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEntity {
    rule: MatchingRule,
}

impl RuleEntity {
    pub fn new(rule: MatchingRule) -> Result<Self, DomainError> {
        validate_rule(&rule)?;
        Ok(Self { rule })
    }

    pub fn read_data(&self) -> &MatchingRule {
        &self.rule
    }

    pub fn into_snapshot(self) -> MatchingRule {
        self.rule
    }

    pub fn read_data_mut(&mut self) -> &mut MatchingRule {
        &mut self.rule
    }

    pub async fn activate(
        &mut self,
        writer: &dyn RuleWriterCap,
        space_id: SpaceId,
    ) -> Result<(), DomainError> {
        if self.rule.active {
            return Ok(());
        }
        writer
            .write_rule(
                ("space", space_id.0),
                &self.rule.id,
                &self.rule.name,
                self.rule.order,
                &self.rule.pattern,
                true,
            )
            .await?;
        self.rule.active = true;
        Ok(())
    }

    pub async fn deactivate(
        &mut self,
        writer: &dyn RuleWriterCap,
        space_id: SpaceId,
    ) -> Result<(), DomainError> {
        if !self.rule.active {
            return Ok(());
        }
        writer
            .write_rule(
                ("space", space_id.0),
                &self.rule.id,
                &self.rule.name,
                self.rule.order,
                &self.rule.pattern,
                false,
            )
            .await?;
        self.rule.active = false;
        Ok(())
    }
}

pub(crate) fn validate_rule_list(rules: &[MatchingRule]) -> Result<(), DomainError> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_active_orders = std::collections::HashSet::new();

    for rule in rules {
        validate_rule(rule)?;
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

fn validate_rule(rule: &MatchingRule) -> Result<(), DomainError> {
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
    Regex::new(&rule.pattern)
        .map_err(|_| DomainError::InvariantViolation("matching rule pattern is invalid"))?;
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
        let entity = RuleEntity::new(sample_rule("a", "ANi", 1, r"^\[ANi\].*$")).expect("entity");

        assert_eq!(entity.read_data().id.0, "a");
    }

    #[test]
    fn new_rejects_invalid_regex() {
        let error = RuleEntity::new(sample_rule("a", "broken", 1, "[")).expect_err("invalid regex");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: matching rule pattern is invalid"
        );
    }

    #[test]
    fn rule_list_validation_rejects_duplicate_order() {
        let error = validate_rule_list(&[
            sample_rule("a", "ANi", 1, r"^\[ANi\].*$"),
            sample_rule("b", "Lilith", 1, r"^\[Lilith\].*$"),
        ])
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

        validate_rule_list(&[sample_rule("a", "ANi", 1, r"^\[ANi\].*$"), inactive])
            .expect("inactive duplicate order");
    }

    #[test]
    fn match_title_uses_rule_regex() {
        let entity = RuleEntity::new(sample_rule("ani", "ANi", 1, r"^\[ANi\].*")).expect("entity");
        let title = snapshot_title("[ANi]");

        let matched = entity
            .match_title(&TestRegexProvider, &title)
            .expect("match")
            .expect("matched");

        assert_eq!(matched.id.0, "ani");
    }
}
