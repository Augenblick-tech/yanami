use async_trait::async_trait;

use crate::shared::error::DomainError;

use super::MatchingRuleId;

#[async_trait]
pub trait RuleWriterCap: Send + Sync {
    async fn write_rule(
        &self,
        scope: (&str, i64),
        rule_id: &MatchingRuleId,
        name: &str,
        order: u32,
        pattern: &str,
        active: bool,
    ) -> Result<(), DomainError>;
}
