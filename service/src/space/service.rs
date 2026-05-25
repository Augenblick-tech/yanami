use std::sync::Arc;

use domain::{shared::error::DomainError, space::SpaceId, user::UserId};
use space::Spaces;

use crate::shared::error::ApplicationError;

pub struct SpaceService {
    spaces: Arc<Spaces>,
}

impl SpaceService {
    pub fn new(spaces: Arc<Spaces>) -> Self {
        Self { spaces }
    }

    pub async fn resolve_personal_space(
        &self,
        user_id: UserId,
    ) -> Result<SpaceId, ApplicationError> {
        let space =
            self.spaces
                .load_personal(user_id)
                .await?
                .ok_or(DomainError::InvariantViolation(
                    "user has no personal space",
                ))?;
        Ok(space.read_data().id)
    }

    pub async fn get_auto_subscribe(&self, space_id: SpaceId) -> Result<bool, ApplicationError> {
        let space = self.spaces.load(space_id).await?;
        Ok(space.read_data().auto_subscribe)
    }

    pub async fn set_auto_subscribe(
        &self,
        space_id: SpaceId,
        enabled: bool,
    ) -> Result<bool, ApplicationError> {
        self.spaces.set_auto_subscribe(space_id, enabled).await?;
        Ok(enabled)
    }
}
