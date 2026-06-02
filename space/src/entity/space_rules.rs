use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{
    cap::{RegexCaps, SpaceRulesContextCaps},
    rule_entity::RuleEntity,
};

/// 订阅空间规则关联对象
#[derive(Clone)]
pub struct SpaceRules {
    space_id: u32,
    cap: Arc<dyn SpaceRulesContextCaps>,
    regex_cap: Arc<dyn RegexCaps>,
}

impl SpaceRules {
    pub(crate) fn new(
        cap: Arc<dyn SpaceRulesContextCaps>,
        regex_cap: Arc<dyn RegexCaps>,
        space_id: u32,
    ) -> Self {
        Self {
            space_id,
            cap: cap,
            regex_cap,
        }
    }

    pub async fn list(&self) -> Result<Vec<RuleEntity>, DomainError> {
        let rule_entity_list = self.cap.list_space_rules(self.space_id).await?;
        let rule_entity_list = rule_entity_list
            .into_iter()
            .filter(|i| i.is_active())
            .collect();
        Ok(rule_entity_list)
    }

    pub async fn new_rule(
        &self,
        name: String,
        pattern: String,
        order: u32,
    ) -> Result<RuleEntity, DomainError> {
        let id = self.cap.next_id().await?;
        let entity = RuleEntity::new(
            id,
            self.space_id,
            name,
            order,
            pattern,
            true,
            self.regex_cap.as_ref(),
        )?;
        Ok(entity)
    }

    pub async fn add_rule(&self, entity: &RuleEntity) -> Result<(), DomainError> {
        self.cap.insert_space_rule(&entity).await?;
        Ok(())
    }

    pub async fn del_rule(
        &self,
        entity: &mut RuleEntity,
        is_referenced: bool,
    ) -> Result<(), DomainError> {
        if is_referenced {
            entity.inactive(self.cap.as_ref()).await?;
            return Ok(());
        }
        self.cap
            .delete_space_rule(self.space_id, entity.id())
            .await?;
        Ok(())
    }
}
