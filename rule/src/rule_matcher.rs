use domain::{rule::MatchingRule, shared::error::DomainError};

pub struct RuleMatcher<'a> {
    rules: &'a [MatchingRule],
    regex_provider: &'a dyn domain::rule::RegexProvider,
}

impl<'a> RuleMatcher<'a> {
    pub fn new(
        rules: &'a [MatchingRule],
        regex_provider: &'a dyn domain::rule::RegexProvider,
    ) -> Self {
        Self {
            rules,
            regex_provider,
        }
    }

    pub fn match_title(&self, title: &str) -> Result<Option<&'a MatchingRule>, DomainError> {
        for rule in self.rules {
            if self.regex_provider.is_match(&rule.pattern, title)? {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }
}
