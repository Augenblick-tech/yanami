use std::sync::Arc;

use common::shared::error::Error;

use crate::entity::{
    cap::{RuleMatcher, RuleRepository},
    model::{Rule, RuleQuery},
    rule_entity::RuleEntity,
};

#[derive(Clone)]
pub struct Rules {
    repo: Arc<dyn RuleRepository>,
    matcher: Arc<dyn RuleMatcher>,
}

impl Rules {
    pub fn new(repo: Arc<dyn RuleRepository>, matcher: Arc<dyn RuleMatcher>) -> Self {
        Self { repo, matcher }
    }

    pub async fn create(
        &self,
        name: &str,
        space_id: i64,
        pattern: &str,
        order: i64,
    ) -> Result<RuleEntity, Error> {
        let rule = Rule {
            space_id,
            name: name.to_string(),
            order,
            pattern: pattern.to_string(),
        };
        self.matcher.validate(&rule.pattern).map_err(|e| {
            Error::external(
                format!("vaildate pattern failed, pattern {}", &rule.pattern),
                e,
            )
        })?;

        let data = self
            .repo
            .insert(&rule)
            .await
            .map_err(|e| Error::external("rules create rule failed", e))?;
        let entity = RuleEntity::new(data, self.matcher.clone());
        Ok(entity)
    }

    pub async fn list(&self, query: &RuleQuery) -> Result<Vec<RuleEntity>, Error> {
        let rule_list = self
            .repo
            .list(query)
            .await
            .map_err(|e| Error::external(format!("list rules failed, query {:?}", query), e))?;
        Ok(rule_list
            .into_iter()
            .map(|i| RuleEntity::new(i, self.matcher.clone()))
            .collect())
    }

    pub async fn find(&self, id: i64) -> Result<Option<RuleEntity>, Error> {
        let rule = self
            .repo
            .find(id)
            .await
            .map_err(|e| Error::external(format!("find rule failed, id {}", id), e))?;
        if let Some(rule) = rule {
            Ok(Some(RuleEntity::new(rule, self.matcher.clone())))
        } else {
            Ok(None)
        }
    }

    pub async fn save(&self, entity: &RuleEntity) -> Result<(), Error> {
        self.repo.save(entity.get_base_data()).await.map_err(|e| {
            Error::external(format!("save rule failed, {:?}", entity.get_base_data()), e)
        })
    }

    pub async fn delete(&self, entity: &RuleEntity) -> Result<(), Error> {
        self.repo
            .delete(entity.id())
            .await
            .map_err(|e| Error::external(format!("delete rule failed, id {}", entity.id()), e))
    }
}
