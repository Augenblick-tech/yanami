use domain::{rule::MatchingRule, rule::RegexProvider, shared::error::DomainError};

use crate::entity::RuleEntity;

impl RuleEntity {
    pub fn match_title(
        &self,
        regex_provider: &dyn RegexProvider,
        title: &str,
    ) -> Result<Option<MatchingRule>, DomainError> {
        let rule = self.read_data();
        if regex_provider.is_match(&rule.pattern, title)? {
            Ok(Some(rule.clone()))
        } else {
            Ok(None)
        }
    }
}
