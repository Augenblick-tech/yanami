use std::sync::Arc;

use domain::{
    shared::biz::BizFactory,
    space::SpaceId,
    system::SystemInfrastructureInitializer,
    user::UserId,
};
use space::Spaces;
use user::users::Users;

use crate::shared::error::ApplicationError;

/// 系统初始化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemInitializationOutcome {
    /// 管理员用户标识。
    pub admin_user_id: UserId,
    /// 管理员个人空间标识。
    pub admin_space_id: SpaceId,
    /// 管理员是否由本次初始化创建。
    pub admin_created: bool,
}

/// 系统初始化应用服务。
#[derive(Clone)]
pub struct SystemService {
    biz: Arc<dyn BizFactory>,
    infrastructure: Arc<dyn SystemInfrastructureInitializer>,
    users: Arc<Users>,
    spaces: Arc<Spaces>,
}

impl SystemService {
    /// 构造系统初始化应用服务。
    pub fn new(
        biz: Arc<dyn BizFactory>,
        infrastructure: Arc<dyn SystemInfrastructureInitializer>,
        users: Arc<Users>,
        spaces: Arc<Spaces>,
    ) -> Self {
        Self {
            biz,
            infrastructure,
            users,
            spaces,
        }
    }

    /// 确保系统基础业务数据存在。
    pub async fn ensure_initialized(
        &self,
        admin_username: &str,
        admin_password: &str,
    ) -> Result<SystemInitializationOutcome, ApplicationError> {
        let biz = self.biz.open_biz().await?;
        let result: Result<SystemInitializationOutcome, ApplicationError> = async {
            tracing::info!("system initialization started");
            tracing::info!("system infrastructure initialization started");
            self.infrastructure.initialize_infrastructure(&biz).await?;
            tracing::info!("system infrastructure initialization finished");
            let users = self.users.with_biz(&biz)?;
            let spaces = self.spaces.with_biz(&biz)?;
            let (admin_user_id, admin_created) =
                if let Some(existing) = users.try_load_by_username(admin_username).await? {
                    tracing::info!(
                        admin_user_id = existing.id().0,
                        "system admin user already exists"
                    );
                    (existing.id(), false)
                } else {
                    let created = users.create_admin(admin_username, admin_password).await?;
                    tracing::info!(admin_user_id = created.id().0, "system admin user created");
                    (created.id(), true)
                };
            tracing::info!(
                admin_user_id = admin_user_id.0,
                "system personal space ensure started"
            );
            let admin_space = spaces.ensure_personal_space(admin_user_id, true).await?;
            tracing::info!(
                admin_user_id = admin_user_id.0,
                admin_space_id = admin_space.read_data().id.0,
                "system personal space ensure finished"
            );
            Ok(SystemInitializationOutcome {
                admin_user_id,
                admin_space_id: admin_space.read_data().id,
                admin_created,
            })
        }
        .await;

        match result {
            Ok(outcome) => {
                if let Err(error) = biz.commit().await {
                    tracing::error!(
                        error = %error,
                        "system initialization commit failed, attempting rollback"
                    );
                    if let Err(rollback_error) = biz.rollback().await {
                        tracing::error!(
                            error = %rollback_error,
                            commit_error = %error,
                            "system initialization rollback after commit failure failed"
                        );
                        return Err(rollback_error.into());
                    }
                    tracing::info!("system initialization rolled back after commit failure");
                    return Err(error.into());
                }
                tracing::info!(
                    admin_user_id = outcome.admin_user_id.0,
                    admin_space_id = outcome.admin_space_id.0,
                    admin_created = outcome.admin_created,
                    "system initialization committed"
                );
                Ok(outcome)
            }
            Err(error) => {
                tracing::error!(
                    error = %error,
                    "system initialization failed, rolling back"
                );
                if let Err(rollback_error) = biz.rollback().await {
                    tracing::error!(
                        error = %rollback_error,
                        original_error = %error,
                        "system initialization rollback failed"
                    );
                    return Err(rollback_error.into());
                }
                tracing::info!("system initialization rolled back");
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use ::user::gateway::{PasswordService, UserIdGenerator};
    use async_trait::async_trait;
    use domain::{
        shared::{
            biz::{BizContext, InfraTxProvider},
            error::DomainError,
            identifier::IdSequence,
        },
        space::{PersonalSpaceBinding, Space, SpaceRepository},
        user::{PasswordHash, User, UserRepository, UserRole, Username},
    };

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

    #[derive(Clone)]
    struct IncrementingUserIds {
        next: Arc<Mutex<i64>>,
    }

    impl IncrementingUserIds {
        fn new(next: i64) -> Self {
            Self {
                next: Arc::new(Mutex::new(next)),
            }
        }
    }

    #[async_trait]
    impl UserIdGenerator for IncrementingUserIds {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn UserIdGenerator>, DomainError> {
            Ok(Arc::new(self.clone()))
        }

        async fn next_user_id(&self) -> Result<UserId, DomainError> {
            let mut next = self.next.lock().expect("lock user ids");
            let id = *next;
            *next += 1;
            Ok(UserId(id))
        }
    }

    #[derive(Clone, Default)]
    struct InMemorySpaces {
        spaces: Arc<Mutex<HashMap<SpaceId, Space>>>,
        bindings: Arc<Mutex<HashMap<UserId, PersonalSpaceBinding>>>,
    }

    #[async_trait]
    impl SpaceRepository for InMemorySpaces {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn SpaceRepository>, DomainError> {
            Ok(Arc::new(self.clone()))
        }

        async fn save_subscription_space(&self, space: &Space) -> Result<(), DomainError> {
            self.spaces
                .lock()
                .expect("lock spaces")
                .insert(space.id, space.clone());
            Ok(())
        }

        async fn find_subscription_space(
            &self,
            space_id: SpaceId,
        ) -> Result<Option<Space>, DomainError> {
            Ok(self
                .spaces
                .lock()
                .expect("lock spaces")
                .get(&space_id)
                .cloned())
        }

        async fn find_personal_space_binding(
            &self,
            user_id: UserId,
        ) -> Result<Option<PersonalSpaceBinding>, DomainError> {
            Ok(self
                .bindings
                .lock()
                .expect("lock bindings")
                .get(&user_id)
                .cloned())
        }

        async fn save_personal_space_binding(
            &self,
            user_id: UserId,
            binding: &PersonalSpaceBinding,
        ) -> Result<(), DomainError> {
            self.bindings
                .lock()
                .expect("lock bindings")
                .insert(user_id, binding.clone());
            Ok(())
        }

        async fn list_auto_subscribing_spaces(&self) -> Result<Vec<Space>, DomainError> {
            let spaces = self.spaces.lock().expect("lock spaces");
            Ok(spaces
                .values()
                .filter(|s| s.auto_subscribe)
                .cloned()
                .collect())
        }

        async fn find_personal_space_user_ids(
            &self,
            space_ids: &[SpaceId],
        ) -> Result<Vec<(SpaceId, UserId)>, DomainError> {
            let bindings = self.bindings.lock().expect("lock bindings");
            let space_set: std::collections::HashSet<SpaceId> = space_ids.iter().copied().collect();
            Ok(bindings
                .iter()
                .filter(|(_, b)| space_set.contains(&b.personal_space_id))
                .map(|(uid, b)| (b.personal_space_id, *uid))
                .collect())
        }
    }

    #[derive(Clone)]
    struct IncrementingSpaceIds {
        next: Arc<Mutex<i64>>,
    }

    impl IncrementingSpaceIds {
        fn new(next: i64) -> Self {
            Self {
                next: Arc::new(Mutex::new(next)),
            }
        }
    }

    #[async_trait]
    impl IdSequence for IncrementingSpaceIds {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn IdSequence>, DomainError> {
            Ok(Arc::new(self.clone()))
        }

        async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError> {
            let mut next = self.next.lock().expect("lock space ids");
            let id = *next;
            *next += 1;
            Ok(SpaceId(id))
        }
    }

    #[derive(Default)]
    struct BizStats {
        commits: Mutex<usize>,
        rollbacks: Mutex<usize>,
    }

    struct RecordingInfrastructureInitializer {
        calls: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl SystemInfrastructureInitializer for RecordingInfrastructureInitializer {
        async fn initialize_infrastructure(&self, _biz: &BizContext) -> Result<(), DomainError> {
            *self.calls.lock().expect("lock infrastructure calls") += 1;
            Ok(())
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

    struct NoopUserPasswordChanger;

    #[async_trait]
    impl domain::user::capability::UserPasswordChangerCap for NoopUserPasswordChanger {
        async fn write_password(
            &self,
            _user_id: UserId,
            _new_hash: String,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct NoopSpaceAutoSubscriber;

    #[async_trait]
    impl domain::space::capability::SpaceAutoSubscribeCap for NoopSpaceAutoSubscriber {
        async fn write_auto_subscribe(
            &self,
            _space_id: SpaceId,
            _auto_subscribe: bool,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct RecordingBizProvider {
        stats: Arc<BizStats>,
    }

    #[async_trait]
    impl InfraTxProvider for RecordingBizProvider {
        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }

        async fn commit(&self) -> Result<(), DomainError> {
            *self.stats.commits.lock().expect("lock commits") += 1;
            Ok(())
        }

        async fn rollback(&self) -> Result<(), DomainError> {
            *self.stats.rollbacks.lock().expect("lock rollbacks") += 1;
            Ok(())
        }
    }

    #[derive(Clone)]
    struct RecordingBizFactory {
        stats: Arc<BizStats>,
    }

    #[async_trait]
    impl BizFactory for RecordingBizFactory {
        async fn open_biz(&self) -> Result<BizContext, DomainError> {
            Ok(BizContext::new(
                0,
                Arc::new(RecordingBizProvider {
                    stats: self.stats.clone(),
                }),
            ))
        }
    }

    fn build_service(
        users: Arc<InMemoryUsers>,
        spaces: Arc<InMemorySpaces>,
        stats: Arc<BizStats>,
        infrastructure_calls: Arc<Mutex<usize>>,
    ) -> SystemService {
        let user_repository: Arc<dyn UserRepository> = users;
        let user_accounts = Arc::new(Users::new(
            user_repository,
            Arc::new(FixedPasswordService),
            Arc::new(IncrementingUserIds::new(10001)),
            user::users::UserCaps {
                info_writer: Arc::new(NoopUserInfoWriter),
                password_changer: Arc::new(NoopUserPasswordChanger),
            },
        ));
        let space_repository: Arc<dyn SpaceRepository> = spaces;
        let spaces = Arc::new(Spaces::new(
            space_repository,
            Arc::new(IncrementingSpaceIds::new(7)),
            space::SpaceCaps {
                auto_subscriber: Arc::new(NoopSpaceAutoSubscriber),
            },
        ));
        SystemService::new(
            Arc::new(RecordingBizFactory { stats }),
            Arc::new(RecordingInfrastructureInitializer {
                calls: infrastructure_calls,
            }),
            user_accounts,
            spaces,
        )
    }

    #[tokio::test]
    async fn ensure_initialized_creates_admin_and_personal_space_once() {
        let users = Arc::new(InMemoryUsers::default());
        let spaces = Arc::new(InMemorySpaces::default());
        let stats = Arc::new(BizStats::default());
        let infrastructure_calls = Arc::new(Mutex::new(0));
        let service = build_service(
            users.clone(),
            spaces.clone(),
            stats.clone(),
            infrastructure_calls.clone(),
        );

        let first = service
            .ensure_initialized("moexco", "123456")
            .await
            .expect("first init");
        let second = service
            .ensure_initialized("moexco", "123456")
            .await
            .expect("second init");

        assert!(first.admin_created);
        assert!(!second.admin_created);
        assert_eq!(first.admin_user_id, UserId(10001));
        assert_eq!(first.admin_space_id, SpaceId(7));
        assert_eq!(second.admin_user_id, UserId(10001));
        assert_eq!(second.admin_space_id, SpaceId(7));
        assert_ne!(first.admin_user_id.0, first.admin_space_id.0);
        assert_eq!(users.items.lock().expect("lock users").len(), 1);
        assert_eq!(spaces.spaces.lock().expect("lock spaces").len(), 1);
        assert_eq!(
            spaces
                .bindings
                .lock()
                .expect("lock bindings")
                .get(&UserId(10001))
                .cloned(),
            Some(PersonalSpaceBinding {
                personal_space_id: SpaceId(7),
            })
        );
        assert_eq!(
            users
                .items
                .lock()
                .expect("lock users")
                .get(&UserId(10001))
                .cloned(),
            Some(User {
                id: UserId(10001),
                username: Username("moexco".to_string()),
                password_hash: PasswordHash("hashed:123456".to_string()),
                role: UserRole::Admin,
            })
        );
        assert_eq!(*stats.commits.lock().expect("lock commits"), 2);
        assert_eq!(*stats.rollbacks.lock().expect("lock rollbacks"), 0);
        assert_eq!(
            *infrastructure_calls
                .lock()
                .expect("lock infrastructure calls"),
            2
        );
    }

    #[tokio::test]
    async fn ensure_initialized_rolls_back_when_admin_is_invalid() {
        let users = Arc::new(InMemoryUsers::default());
        let spaces = Arc::new(InMemorySpaces::default());
        let stats = Arc::new(BizStats::default());
        let infrastructure_calls = Arc::new(Mutex::new(0));
        let service = build_service(
            users.clone(),
            spaces.clone(),
            stats.clone(),
            infrastructure_calls.clone(),
        );

        let error = service
            .ensure_initialized("moexco", "bad")
            .await
            .expect_err("password too short");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: password must be at least 6 characters"
        );
        assert!(users.items.lock().expect("lock users").is_empty());
        assert!(spaces.spaces.lock().expect("lock spaces").is_empty());
        assert!(spaces.bindings.lock().expect("lock bindings").is_empty());
        assert_eq!(*stats.commits.lock().expect("lock commits"), 0);
        assert_eq!(*stats.rollbacks.lock().expect("lock rollbacks"), 1);
        assert_eq!(
            *infrastructure_calls
                .lock()
                .expect("lock infrastructure calls"),
            1
        );
    }
}
