use async_trait::async_trait;
use domain::shared::error::DomainError;

use crate::entity::{rule_entity::RuleEntity, space_entity::SpaceEntity};

#[async_trait]
pub trait SpaceRulesCaps: Send + Sync {
    async fn list_space_rules(&self, space_id: u32) -> Result<Vec<RuleEntity>, DomainError>;
    async fn insert_space_rule(&self, entity: &RuleEntity) -> Result<(), DomainError>;
    async fn delete_space_rule(&self, space_id: u32, rule_id: u32) -> Result<(), DomainError>;
}

pub trait RegexCaps: Send + Sync {
    fn is_match(&self, pattern: &str, text: &str) -> Result<bool, DomainError>;
    fn verify(&self, pattern: &str) -> Result<(), DomainError>;
    fn delete_pattern(&self, pattern: &str);
}

#[async_trait]
pub trait SpaceRepository: Send + Sync + SpaceRulesContextCaps {
    async fn find_by_space_id(&self, space_id: u32) -> Result<Option<SpaceEntity>, DomainError>;
    async fn find_by_user_id(&self, user_id: u32) -> Result<Option<SpaceEntity>, DomainError>;
}

#[async_trait]
pub trait SpaceAutoSubcribeCaps: Send + Sync {
    async fn set_auto_subcribe(&self, space_id: u32, auto_sub: bool) -> Result<(), DomainError>;
}

#[async_trait]
pub trait UpdateRuleCaps: Send + Sync {
    async fn update_pattern(&self, rule_id: u32, pattern: &str, old_pattern: &str) -> Result<(), DomainError>;
    async fn update_order(&self, rule_id: u32, order: u32) -> Result<(), DomainError>;
    async fn inactive(&self, rule_id: u32) -> Result<(), DomainError>;
}

#[async_trait]
pub trait SpaceRulesContextCaps: Send + Sync + UpdateRuleCaps + SpaceRulesCaps + RuleIDGenerator {}

#[async_trait]
pub trait RuleIDGenerator: Send + Sync {
    async fn next_id(&self) -> Result<u32, DomainError>;
}
