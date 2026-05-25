use std::sync::Arc;

use domain::user::{AccessToken, AccessTokenIssuer, User, UserId, UserRole};
use user::users::Users;

use crate::shared::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginOutcome {
    pub user_id: UserId,
    pub role: UserRole,
    pub access_token: AccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePasswordOutcome {
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListUsersOutcome {
    pub users: Vec<User>,
}

#[derive(Clone)]
pub struct UserService {
    users: Arc<Users>,
    access_tokens: Arc<dyn AccessTokenIssuer>,
}

impl UserService {
    pub fn new(users: Arc<Users>, access_tokens: Arc<dyn AccessTokenIssuer>) -> Self {
        Self {
            users,
            access_tokens,
        }
    }

    pub async fn login(
        &self,
        username: String,
        password: String,
    ) -> Result<LoginOutcome, ApplicationError> {
        let user = self.users.load_by_username(&username).await?;
        self.users.verify_password(user.id(), &password).await?;
        let access_token = self
            .access_tokens
            .issue_access_token(user.id(), user.role())
            .await?;
        Ok(LoginOutcome {
            user_id: user.id(),
            role: user.role(),
            access_token,
        })
    }

    pub async fn change_password(
        &self,
        user_id: UserId,
        old_password: String,
        new_password: String,
    ) -> Result<ChangePasswordOutcome, ApplicationError> {
        self.users
            .change_password(user_id, &old_password, &new_password)
            .await?;
        Ok(ChangePasswordOutcome { user_id })
    }

    pub async fn list_users(&self) -> Result<ListUsersOutcome, ApplicationError> {
        Ok(ListUsersOutcome {
            users: self
                .users
                .list()
                .await?
                .into_iter()
                .map(|user| user.into_snapshot())
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::{
        shared::{biz::BizContext, error::DomainError},
        user::{PasswordHash, User, UserRepository, UserRole, Username},
    };
    use user::gateway::{PasswordService, UserIdGenerator};

    use super::*;

    #[derive(Clone, Default)]
    struct InMemoryUsers {
        items: Arc<Mutex<HashMap<UserId, User>>>,
    }

    #[async_trait]
    impl UserRepository for InMemoryUsers {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn UserRepository>, DomainError> {
            Ok(Arc::new(self.clone()))
        }

        async fn find_user(&self, user_id: UserId) -> Result<Option<User>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lock users")
                .get(&user_id)
                .cloned())
        }

        async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lock users")
                .values()
                .find(|user| user.username.0 == username)
                .cloned())
        }

        async fn save_user(&self, user: &User) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("lock users")
                .insert(user.id, user.clone());
            Ok(())
        }

        async fn list_users(&self) -> Result<Vec<User>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("lock users")
                .values()
                .cloned()
                .collect())
        }
    }

    struct FixedPasswordService;

    #[async_trait]
    impl PasswordService for FixedPasswordService {
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

    #[derive(Clone, Default)]
    struct FixedUserIdGenerator;

    #[async_trait]
    impl UserIdGenerator for FixedUserIdGenerator {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn UserIdGenerator>, DomainError> {
            Ok(Arc::new(Self))
        }

        async fn next_user_id(&self) -> Result<UserId, DomainError> {
            Ok(UserId(11))
        }
    }

    struct FixedTokenIssuer;

    #[async_trait]
    impl AccessTokenIssuer for FixedTokenIssuer {
        async fn issue_access_token(
            &self,
            user_id: UserId,
            _role: UserRole,
        ) -> Result<AccessToken, DomainError> {
            Ok(AccessToken {
                access_token: format!("token-{}", user_id.0),
                token_type: "Bearer".to_string(),
                expires_at: 999,
            })
        }
    }

    struct NoopUserInfoWriter;

    #[async_trait]
    impl domain::user::capability::UserInfoWriterCap for NoopUserInfoWriter {
        async fn write_info(
            &self,
            _user_id: UserId,
            _info: &domain::user::capability::UserInfoUpdate,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct NoopUserPasswordChanger {
        users: Arc<InMemoryUsers>,
    }

    #[async_trait]
    impl domain::user::capability::UserPasswordChangerCap for NoopUserPasswordChanger {
        async fn write_password(
            &self,
            user_id: UserId,
            new_hash: String,
        ) -> Result<(), DomainError> {
            let users = self.users.items.lock().expect("lock users");
            if let Some(user) = users.get(&user_id) {
                let mut updated = user.clone();
                updated.password_hash = PasswordHash(new_hash);
                drop(users);
                self.users.items.lock().expect("lock users").insert(user_id, updated);
            }
            Ok(())
        }
    }

    fn build_users(users: Arc<InMemoryUsers>) -> Arc<user::users::Users> {
        let repository: Arc<dyn UserRepository> = users.clone();
        Arc::new(user::users::Users::new(
            repository,
            Arc::new(FixedPasswordService),
            Arc::new(FixedUserIdGenerator),
            user::users::UserCaps {
                info_writer: Arc::new(NoopUserInfoWriter),
                password_changer: Arc::new(NoopUserPasswordChanger {
                    users: users.clone(),
                }),
            },
        ))
    }

    #[tokio::test]
    async fn login_change_password_and_list_users() {
        let users = Arc::new(InMemoryUsers::default());
        users
            .save_user(&User {
                id: UserId(1),
                username: Username("admin".to_string()),
                password_hash: PasswordHash("hashed:pw".to_string()),
                role: UserRole::Admin,
            })
            .await
            .expect("save admin");
        users
            .save_user(&User {
                id: UserId(2),
                username: Username("alice".to_string()),
                password_hash: PasswordHash("hashed:pw".to_string()),
                role: UserRole::User,
            })
            .await
            .expect("save user");
        let user_accounts = build_users(users.clone());
        let service = UserService::new(user_accounts, Arc::new(FixedTokenIssuer));

        let login = service
            .login("alice".to_string(), "pw".to_string())
            .await
            .expect("login");
        let change = service
            .change_password(UserId(2), "pw".to_string(), "next123".to_string())
            .await
            .expect("change");
        let users_list = service.list_users().await.expect("list");

        assert_eq!(login.user_id, UserId(2));
        assert_eq!(login.access_token.access_token, "token-2");
        assert_eq!(change.user_id, UserId(2));
        assert_eq!(users_list.users.len(), 2);
        assert_eq!(
            users
                .find_user(UserId(2))
                .await
                .expect("find user")
                .expect("user")
                .password_hash,
            PasswordHash("hashed:next123".to_string())
        );
    }
}
