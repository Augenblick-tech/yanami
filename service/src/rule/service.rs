use std::sync::Arc;

use domain::{
    rule::{MatchingRule, MatchingRuleId},
    space::SpaceId,
};

use crate::shared::error::ApplicationError;

type InvalidateSpaceRules = dyn Fn(SpaceId) + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRulesOutcome {
    pub space_id: SpaceId,
    pub rules: Vec<MatchingRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRuleOutcome {
    pub space_id: SpaceId,
    pub rule: MatchingRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRuleOutcome {
    pub space_id: SpaceId,
    pub rule_id: MatchingRuleId,
}

pub struct RuleService {
    rules: Arc<rule::Rules>,
    invalidate_space_rules: Arc<InvalidateSpaceRules>,
}

impl RuleService {
    pub fn new(rules: Arc<rule::Rules>, invalidate_space_rules: Arc<InvalidateSpaceRules>) -> Self {
        Self {
            rules,
            invalidate_space_rules,
        }
    }

    pub async fn get_rules(&self, space_id: SpaceId) -> Result<GetRulesOutcome, ApplicationError> {
        Ok(GetRulesOutcome {
            space_id,
            rules: self
                .rules
                .list(space_id)
                .await?
                .into_iter()
                .map(|rule| rule.into_snapshot())
                .collect(),
        })
    }

    pub async fn save_rule(
        &self,
        space_id: SpaceId,
        rule: MatchingRule,
    ) -> Result<SaveRuleOutcome, ApplicationError> {
        let rule_entity = self.rules.create(rule).map_err(ApplicationError::from)?;
        let saved = self.rules.save_rule(space_id, rule_entity).await?;
        (self.invalidate_space_rules)(space_id);
        Ok(SaveRuleOutcome {
            space_id,
            rule: saved.into_snapshot(),
        })
    }

    pub async fn delete_rule(
        &self,
        space_id: SpaceId,
        rule_id: MatchingRuleId,
    ) -> Result<DeleteRuleOutcome, ApplicationError> {
        let deleted = self.rules.deactivate_rule(space_id, &rule_id).await?;
        (self.invalidate_space_rules)(space_id);
        Ok(DeleteRuleOutcome {
            space_id,
            rule_id: deleted.into_snapshot().id,
        })
    }
}
