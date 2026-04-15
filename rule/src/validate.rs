use domain::{rule::MatchingRule, shared::error::DomainError};

use crate::entity::RuleEntity;

impl RuleEntity {
    pub fn replace_rule(&mut self, rule: MatchingRule) -> Result<(), DomainError> {
        *self = Self::new(rule)?;
        Ok(())
    }
}
