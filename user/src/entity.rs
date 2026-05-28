use domain::{
    shared::error::DomainError,
    user::capability::UserPasswordChangerCap,
    user::{User, UserId, UserRole, Username},
};
use std::fmt;

use crate::gateway::PasswordService;

#[derive(Clone)]
pub struct UserEntity {
    snapshot: User,
}

impl fmt::Debug for UserEntity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserEntity")
            .field("user_id", &self.snapshot.id)
            .field("username", &self.snapshot.username)
            .field("role", &self.snapshot.role)
            .finish()
    }
}

impl UserEntity {
    /// 基于用户快照创建用户聚合根。
    pub fn new(snapshot: User) -> Self {
        Self { snapshot }
    }

    pub fn read_data(&self) -> &User {
        &self.snapshot
    }

    pub fn into_snapshot(self) -> User {
        self.snapshot
    }

    pub fn id(&self) -> UserId {
        self.snapshot.id
    }

    pub fn role(&self) -> UserRole {
        self.snapshot.role
    }

    pub fn username(&self) -> &Username {
        &self.snapshot.username
    }

    pub async fn verify_password(
        &self,
        password: &str,
        password_service: &dyn PasswordService,
    ) -> Result<(), DomainError> {
        password_service
            .verify_password(&self.snapshot.password_hash, password)
            .await?
            .then_some(())
            .ok_or(DomainError::InvariantViolation("wrong password"))
    }

    pub async fn change_password(
        &mut self,
        old_password: &str,
        new_password: &str,
        password_service: &dyn PasswordService,
        cap: &dyn UserPasswordChangerCap,
    ) -> Result<(), DomainError> {
        validate_password(new_password)?;
        self.verify_password(old_password, password_service).await?;
        let new_hash = password_service.hash_password(new_password).await?;
        cap.write_password(self.snapshot.id, new_hash.0.clone())
            .await?;
        self.snapshot.password_hash = new_hash;
        Ok(())
    }

    pub async fn new_user(
        user_id: UserId,
        username: Username,
        password: &str,
        password_service: &dyn PasswordService,
    ) -> Result<Self, DomainError> {
        validate_username(&username.0)?;
        validate_password(password)?;
        Ok(Self {
            snapshot: User {
                id: user_id,
                username,
                password_hash: password_service.hash_password(password).await?,
                role: UserRole::User,
            },
        })
    }

    pub async fn new_admin(
        user_id: UserId,
        username: Username,
        password: &str,
        password_service: &dyn PasswordService,
    ) -> Result<Self, DomainError> {
        validate_username(&username.0)?;
        validate_password(password)?;
        Ok(Self {
            snapshot: User {
                id: user_id,
                username,
                password_hash: password_service.hash_password(password).await?,
                role: UserRole::Admin,
            },
        })
    }
}

pub fn validate_username(username: &str) -> Result<Username, DomainError> {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvariantViolation("username cannot be empty"));
    }
    Ok(Username(trimmed.to_string()))
}

pub fn validate_password(password: &str) -> Result<(), DomainError> {
    if password.trim().len() < 6 {
        return Err(DomainError::InvariantViolation(
            "password must be at least 6 characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::gateway::PasswordService;
    use domain::user::capability::UserPasswordChangerCap;

    struct StubPasswordService;

    struct NoopPasswordChanger;

    #[async_trait]
    impl UserPasswordChangerCap for NoopPasswordChanger {
        async fn write_password(
            &self,
            _user_id: UserId,
            _new_hash: String,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[async_trait]
    impl PasswordService for StubPasswordService {
        async fn hash_password(
            &self,
            plaintext: &str,
        ) -> Result<domain::user::PasswordHash, DomainError> {
            Ok(domain::user::PasswordHash(format!("hashed:{plaintext}")))
        }

        async fn verify_password(
            &self,
            password_hash: &domain::user::PasswordHash,
            plaintext: &str,
        ) -> Result<bool, DomainError> {
            Ok(password_hash.0 == format!("hashed:{plaintext}"))
        }
    }

    fn sample_user() -> User {
        User {
            id: UserId(7),
            username: Username("alice".to_string()),
            password_hash: domain::user::PasswordHash("hashed:123456".to_string()),
            role: UserRole::User,
        }
    }

    #[tokio::test]
    async fn verify_password_requires_matching_password() {
        let entity = UserEntity::new(sample_user());

        let mismatch = match entity.verify_password("bad", &StubPasswordService).await {
            Ok(_) => panic!("bad password"),
            Err(error) => error,
        };
        entity
            .verify_password("123456", &StubPasswordService)
            .await
            .expect("password matches");

        assert_eq!(
            mismatch.to_string(),
            "domain invariant violation: wrong password"
        );
    }

    #[tokio::test]
    async fn change_password_and_constructors_validate_input() {
        let mut entity = UserEntity::new(sample_user());

        let empty_username = UserEntity::new_user(
            UserId(8),
            Username("   ".to_string()),
            "123456",
            &StubPasswordService,
        )
        .await
        .expect_err("username");
        let empty_password = UserEntity::new_admin(
            UserId(8),
            Username("root".to_string()),
            "   ",
            &StubPasswordService,
        )
        .await
        .expect_err("password");
        let noop_cap = NoopPasswordChanger;
        let wrong_old = match entity
            .change_password("bad", "next123", &StubPasswordService, &noop_cap)
            .await
        {
            Ok(_) => panic!("wrong old password"),
            Err(error) => error,
        };

        entity
            .change_password("123456", "next123", &StubPasswordService, &noop_cap)
            .await
            .expect("change");

        assert_eq!(
            empty_username.to_string(),
            "domain invariant violation: username cannot be empty"
        );
        assert_eq!(
            empty_password.to_string(),
            "domain invariant violation: password must be at least 6 characters"
        );
        assert_eq!(
            wrong_old.to_string(),
            "domain invariant violation: wrong password"
        );
        assert_eq!(entity.read_data().password_hash.0, "hashed:next123");
    }
}
