use std::sync::Arc;

use async_trait::async_trait;
use domain::shared::error::DomainError;

use crate::entity::{
    cap::{RegexCaps, SpaceRulesCaps, UpdateRuleCaps},
    rule_entity::RuleEntity,
};

struct SpaceSqliteRepository {
    regex: Arc<dyn RegexCaps>,
}

#[async_trait]
impl UpdateRuleCaps for SpaceSqliteRepository {
    async fn update_pattern(&self, rule_id: u32, pattern: &str, old_pattern: &str) -> Result<(), DomainError> {
        // 必须清除旧正则的缓存，防止缓存无限增长
        self.regex.delete_pattern(old_pattern);
        todo!()
    }
    async fn update_order(&self, rule_id: u32, order: u32) -> Result<(), DomainError> {
        todo!()
    }
    async fn inactive(&self, rule_id: u32) -> Result<(), DomainError> {
        // 软删除之后需要返回pattern，用于清理缓存中对应的数据
        todo!()
    }
}

#[async_trait]
impl SpaceRulesCaps for SpaceSqliteRepository {
    async fn list_space_rules(&self, space_id: u32) -> Result<Vec<RuleEntity>, DomainError> {
        todo!()
    }
    async fn insert_space_rule(&self, entity: &RuleEntity) -> Result<(), DomainError> {
        todo!()
    }
    async fn delete_space_rule(&self, space_id: u32, rule_id: u32) -> Result<(), DomainError> {
        // 删除之后需要返回pattern，用于清理缓存中对应的数据
        todo!()
    }
}
