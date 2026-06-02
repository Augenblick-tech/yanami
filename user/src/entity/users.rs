use std::sync::Arc;

use domain::shared::error::DomainError;

use crate::entity::{
    cap::UserRepository,
    user_entity::{UserEntity, UserRole},
};

#[derive(Clone)]
pub struct Users {
    repo: Arc<dyn UserRepository>,
}

impl Users {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(&self, username: &str, password: &str) -> Result<UserEntity, DomainError> {
        let id = self.repo.next_id().await?;
        let entity = UserEntity::new(
            id,
            username.to_string(),
            password.to_string(),
            UserRole::User,
        )?;
        self.repo.insert(&entity).await?;

        Ok(entity)
    }

    pub async fn find(&self, user_id: u32) -> Result<Option<UserEntity>, DomainError> {
        Ok(self.repo.find(user_id).await?)
    }

    pub async fn save(&self, user: &UserEntity) -> Result<(), DomainError> {
        Ok(self.repo.update(user).await?)
    }
}
