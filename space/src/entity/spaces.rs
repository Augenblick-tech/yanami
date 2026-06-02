use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{
    cap::{RegexCaps, SpaceRepository},
    space_entity::SpaceEntity,
    space_rules::SpaceRules,
};

pub struct Spaces {
    repo: Arc<dyn SpaceRepository>,
    regex_cap: Arc<dyn RegexCaps>,
}

impl Spaces {
    pub fn new(repo: Arc<dyn SpaceRepository>, regex_cap: Arc<dyn RegexCaps>) -> Self {
        Self { repo, regex_cap }
    }

    pub async fn find_by_space_id(
        &self,
        space_id: u32,
    ) -> Result<Option<SpaceEntity>, DomainError> {
        let entity = self.repo.find_by_space_id(space_id).await?;
        Ok(entity)
    }

    pub async fn find_by_user_id(&self, user_id: u32) -> Result<Option<SpaceEntity>, DomainError> {
        let entity = self.repo.find_by_user_id(user_id).await?;
        Ok(entity)
    }

    pub fn rules_of(&self, entity: &SpaceEntity) -> SpaceRules {
        SpaceRules::new(self.repo.clone(), self.regex_cap.clone(), entity.id())
    }
}
