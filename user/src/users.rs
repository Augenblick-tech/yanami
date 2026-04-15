use std::sync::Arc;

use domain::{
    shared::biz::BizContext,
    shared::error::DomainError,
    user::{User, UserId, UserRepository, Username},
};

use crate::{
    entity::{validate_username, UserEntity},
    gateway::{PasswordService, UserIdGenerator},
};

#[derive(Clone)]
pub struct Users {
    repository: Arc<dyn UserRepository>,
    password_service: Arc<dyn PasswordService>,
    user_ids: Arc<dyn UserIdGenerator>,
}

impl Users {
    /// 构造用户账号集合入口。
    pub fn new(
        repository: Arc<dyn UserRepository>,
        password_service: Arc<dyn PasswordService>,
        user_ids: Arc<dyn UserIdGenerator>,
    ) -> Self {
        Self {
            repository,
            password_service,
            user_ids,
        }
    }

    pub fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            repository: self.repository.with_biz(biz)?,
            password_service: self.password_service.clone(),
            user_ids: self.user_ids.with_biz(biz)?,
        })
    }

    fn build_entity(&self, snapshot: User) -> UserEntity<'_> {
        UserEntity::new(snapshot, self.password_service.as_ref())
    }

    pub async fn load(&self, user_id: UserId) -> Result<UserEntity<'_>, DomainError> {
        let snapshot = self
            .repository
            .find_user(user_id)
            .await?
            .ok_or(DomainError::InvariantViolation("user not found"))?;
        Ok(self.build_entity(snapshot))
    }

    pub async fn try_load_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserEntity<'_>>, DomainError> {
        let username = validate_username(username)?;
        Ok(self
            .repository
            .find_user_by_username(&username.0)
            .await?
            .map(|snapshot| self.build_entity(snapshot)))
    }

    pub async fn create(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserEntity<'_>, DomainError> {
        let username = validate_username(username)?;
        if self
            .repository
            .find_user_by_username(&username.0)
            .await?
            .is_some()
        {
            return Err(DomainError::InvariantViolation("username already exists"));
        }

        let user_id = self.user_ids.next_user_id().await?;
        let entity = UserEntity::new_user(
            user_id,
            Username(username.0),
            password,
            self.password_service.as_ref(),
        )
        .await?;
        self.repository.save_user(entity.read_data()).await?;
        Ok(entity)
    }

    pub async fn create_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserEntity<'_>, DomainError> {
        let username = validate_username(username)?;
        if self
            .repository
            .find_user_by_username(&username.0)
            .await?
            .is_some()
        {
            return Err(DomainError::InvariantViolation("username already exists"));
        }

        let user_id = self.user_ids.next_user_id().await?;
        let entity = UserEntity::new_admin(
            user_id,
            Username(username.0),
            password,
            self.password_service.as_ref(),
        )
        .await?;
        self.repository.save_user(entity.read_data()).await?;
        Ok(entity)
    }

    pub async fn load_by_username(&self, username: &str) -> Result<UserEntity<'_>, DomainError> {
        self.try_load_by_username(username)
            .await?
            .ok_or(DomainError::InvariantViolation("user not found"))
    }

    pub async fn save(&self, entity: &UserEntity<'_>) -> Result<(), DomainError> {
        self.repository.save_user(entity.read_data()).await
    }

    pub async fn list(&self) -> Result<Vec<UserEntity<'_>>, DomainError> {
        Ok(self
            .repository
            .list_users()
            .await?
            .into_iter()
            .map(|snapshot| self.build_entity(snapshot))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use async_trait::async_trait;
    use domain::user::{PasswordHash, User, UserRole};

    use super::*;

    #[derive(Default)]
    struct InMemoryRepository {
        by_id: Mutex<HashMap<UserId, User>>,
    }

    #[async_trait]
    impl UserRepository for InMemoryRepository {
        async fn find_user(&self, user_id: UserId) -> Result<Option<User>, DomainError> {
            Ok(self.by_id.lock().expect("by_id").get(&user_id).cloned())
        }

        async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            Ok(self
                .by_id
                .lock()
                .expect("by_id")
                .values()
                .find(|user| user.username.0 == username)
                .cloned())
        }

        async fn save_user(&self, user: &User) -> Result<(), DomainError> {
            self.by_id
                .lock()
                .expect("by_id")
                .insert(user.id, user.clone());
            Ok(())
        }

        async fn list_users(&self) -> Result<Vec<User>, DomainError> {
            Ok(self
                .by_id
                .lock()
                .expect("by_id")
                .values()
                .cloned()
                .collect())
        }
    }

    struct StubPasswordService;

    #[async_trait]
    impl PasswordService for StubPasswordService {
        async fn hash_password(&self, plaintext: &str) -> Result<PasswordHash, DomainError> {
            Ok(PasswordHash(format!("hashed:{plaintext}")))
        }

        async fn verify_password(
            &self,
            password_hash: &PasswordHash,
            plaintext: &str,
        ) -> Result<bool, DomainError> {
            Ok(password_hash.0 == format!("hashed:{plaintext}"))
        }
    }

    struct FixedUserIdGenerator(i64);

    #[async_trait]
    impl UserIdGenerator for FixedUserIdGenerator {
        async fn next_user_id(&self) -> Result<UserId, DomainError> {
            Ok(UserId(self.0))
        }
    }

    fn users(repository: Arc<InMemoryRepository>, id: i64) -> Users {
        let repository_port: Arc<dyn UserRepository> = repository.clone();
        Users::new(
            repository_port,
            Arc::new(StubPasswordService),
            Arc::new(FixedUserIdGenerator(id)),
        )
    }

    #[tokio::test]
    async fn load_by_username_trims_input_and_missing_load_fails() {
        let repository = Arc::new(InMemoryRepository::default());
        repository
            .save_user(&User {
                id: UserId(7),
                username: Username("alice".to_string()),
                password_hash: PasswordHash("hashed:123456".to_string()),
                role: UserRole::User,
            })
            .await
            .expect("save");
        let users = users(repository, 99);

        let found = users.try_load_by_username("  alice ").await.expect("found");
        let missing = users.load(UserId(404)).await.expect_err("missing");

        assert_eq!(found.expect("user").id(), UserId(7));
        assert_eq!(
            missing.to_string(),
            "domain invariant violation: user not found"
        );
    }

    #[tokio::test]
    async fn create_and_create_admin_validate_duplicate_username() {
        let repository = Arc::new(InMemoryRepository::default());
        let users = users(repository.clone(), 10);

        let created = users.create("alice", "123456").await.expect("create");
        let duplicate = users
            .create("alice", "other12")
            .await
            .expect_err("duplicate must fail");
        let admin = users
            .create_admin("root", "123456")
            .await
            .expect("create admin");

        assert_eq!(created.id(), UserId(10));
        assert_eq!(created.role(), UserRole::User);
        assert_eq!(admin.role(), UserRole::Admin);
        assert_eq!(
            duplicate.to_string(),
            "domain invariant violation: username already exists"
        );
        assert_eq!(
            repository
                .find_user_by_username("root")
                .await
                .expect("find")
                .expect("exists")
                .role,
            UserRole::Admin
        );
    }

    #[tokio::test]
    async fn load_by_username_returns_entity() {
        let repository = Arc::new(InMemoryRepository::default());
        repository
            .save_user(&User {
                id: UserId(7),
                username: Username("alice".to_string()),
                password_hash: PasswordHash("hashed:123456".to_string()),
                role: UserRole::User,
            })
            .await
            .expect("save");
        let users = users(repository, 99);

        let entity = users.load_by_username("  alice ").await.expect("load");

        assert_eq!(entity.id(), UserId(7));
        assert_eq!(entity.role(), UserRole::User);
        entity.verify_password("123456").await.expect("password");
    }

    #[tokio::test]
    async fn save_persists_entity_changes() {
        let repository = Arc::new(InMemoryRepository::default());
        let users = users(repository.clone(), 10);
        users.create("alice", "123456").await.expect("create");

        let mut entity = users.load(UserId(10)).await.expect("load");
        entity
            .change_password("123456", "next123")
            .await
            .expect("change");
        users.save(&entity).await.expect("save");

        let saved = repository
            .find_user(UserId(10))
            .await
            .expect("find")
            .expect("exists");
        assert_eq!(saved.password_hash.0, "hashed:next123");
    }
}
