use std::sync::Arc;

use domain::{shared::error::DomainError, space::{SpaceId, SpaceRepository}};

use crate::rule_entity::RuleEntity;

/// 订阅空间规则聚合根
#[derive(Clone)]
pub struct SpaceRules {
    space_id: SpaceId,
    space_repository: Arc<dyn SpaceRepository>,
}

impl SpaceRules {
    pub fn new(repo: Arc<dyn SpaceRepository>, space_id: SpaceId) -> Self {
        Self { space_id, space_repository: repo }
    }

    pub fn list(&self) -> Result<Vec<RuleEntity>, DomainError> {
        
        Ok(vec![])
    }
}
