use std::collections::HashMap;
use std::sync::Arc;

use crate::space_entity::SpaceEntity;
use domain::{
    shared::{biz::BizContext, error::DomainError, identifier::IdSequence},
    space::{PersonalSpaceBinding, SpaceId, SpaceRepository},
    user::UserId,
};

#[derive(Clone)]
struct SpaceAccess {
    space_repository: Arc<dyn SpaceRepository>,
    identifiers: Arc<dyn IdSequence>,
}

impl SpaceAccess {
    fn new(spaces: Arc<dyn SpaceRepository>, identifiers: Arc<dyn IdSequence>) -> Self {
        Self {
            space_repository: spaces,
            identifiers,
        }
    }

    fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            space_repository: self.space_repository.with_biz(biz)?,
            identifiers: self.identifiers.with_biz(biz)?,
        })
    }

    pub async fn load(&self, space_id: SpaceId) -> Result<SpaceEntity, DomainError> {
        let space = self
            .space_repository
            .find_subscription_space(space_id)
            .await?
            .ok_or(DomainError::InvariantViolation(
                "subscription space not found",
            ))?;
        SpaceEntity::new(space)
    }

    pub async fn save(&self, entity: &SpaceEntity) -> Result<(), DomainError> {
        self.space_repository
            .save_subscription_space(entity.read_data())
            .await
    }

    async fn load_personal(&self, user_id: UserId) -> Result<Option<SpaceEntity>, DomainError> {
        let Some(binding) = self
            .space_repository
            .find_personal_space_binding(user_id)
            .await?
        else {
            return Ok(None);
        };
        self.load(binding.personal_space_id).await.map(Some)
    }

    async fn create_personal(
        &self,
        user_id: UserId,
        auto_subscribe: bool,
    ) -> Result<SpaceEntity, DomainError> {
        let space_id = self.identifiers.next_subscription_space_id().await?;
        let entity = SpaceEntity::personal(space_id, auto_subscribe)?;
        self.save(&entity).await?;
        self.space_repository
            .save_personal_space_binding(
                user_id,
                &PersonalSpaceBinding {
                    personal_space_id: space_id,
                },
            )
            .await?;
        Ok(entity)
    }

    async fn list_auto_subscribing_spaces(&self) -> Result<Vec<SpaceEntity>, DomainError> {
        let spaces = self.space_repository.list_auto_subscribing_spaces().await?;
        spaces.into_iter().map(SpaceEntity::new).collect()
    }

    async fn find_personal_space_user_ids(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<HashMap<SpaceId, UserId>, DomainError> {
        let pairs = self.space_repository.find_personal_space_user_ids(space_ids).await?;
        Ok(pairs.into_iter().collect())
    }
}

#[derive(Clone)]
pub struct Spaces {
    access: Arc<SpaceAccess>,
}

impl Spaces {
    pub fn new(spaces: Arc<dyn SpaceRepository>, identifiers: Arc<dyn IdSequence>) -> Self {
        Self {
            access: Arc::new(SpaceAccess::new(spaces, identifiers)),
        }
    }

    pub fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            access: Arc::new(self.access.with_biz(biz)?),
        })
    }

    pub async fn load(&self, space_id: SpaceId) -> Result<SpaceEntity, DomainError> {
        self.access.load(space_id).await
    }

    pub async fn save(&self, entity: &SpaceEntity) -> Result<(), DomainError> {
        self.access.save(entity).await
    }

    /// 读取用户个人订阅空间。
    pub async fn load_personal(&self, user_id: UserId) -> Result<Option<SpaceEntity>, DomainError> {
        self.access.load_personal(user_id).await
    }

    /// 列出所有启用了自动订阅的空间。
    pub async fn list_auto_subscribing_spaces(&self) -> Result<Vec<SpaceEntity>, DomainError> {
        self.access.list_auto_subscribing_spaces().await
    }

    /// 按空间 ID 批量查出绑定用户。
    pub async fn find_personal_space_user_ids(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<HashMap<SpaceId, UserId>, DomainError> {
        self.access.find_personal_space_user_ids(space_ids).await
    }

    /// 确保用户至少持有一个个人订阅空间。
    pub async fn ensure_personal_space(
        &self,
        user_id: UserId,
        auto_subscribe: bool,
    ) -> Result<SpaceEntity, DomainError> {
        if let Some(entity) = self.load_personal(user_id).await? {
            return Ok(entity);
        }
        self.access.create_personal(user_id, auto_subscribe).await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;

    use domain::space::Space;

    use super::*;

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
            Ok(spaces.values().filter(|s| s.auto_subscribe).cloned().collect())
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
    struct IncrementingIds {
        next: Arc<Mutex<i64>>,
    }

    impl IncrementingIds {
        fn new(next: i64) -> Self {
            Self {
                next: Arc::new(Mutex::new(next)),
            }
        }
    }

    #[async_trait]
    impl IdSequence for IncrementingIds {
        fn with_biz(&self, _: &BizContext) -> Result<Arc<dyn IdSequence>, DomainError> {
            Ok(Arc::new(self.clone()))
        }

        async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError> {
            let mut next = self.next.lock().expect("lock ids");
            let id = *next;
            *next += 1;
            Ok(SpaceId(id))
        }
    }

    #[tokio::test]
    async fn ensure_personal_space_creates_space_from_space_id_sequence() {
        let repository = Arc::new(InMemorySpaces::default());
        let spaces = Spaces::new(repository.clone(), Arc::new(IncrementingIds::new(42)));

        let entity = spaces
            .ensure_personal_space(UserId(10001), false)
            .await
            .expect("ensure personal space");

        assert_eq!(entity.read_data().id, SpaceId(42));
        assert!(!entity.read_data().auto_subscribe);
        assert_eq!(
            repository
                .bindings
                .lock()
                .expect("lock bindings")
                .get(&UserId(10001))
                .cloned(),
            Some(PersonalSpaceBinding {
                personal_space_id: SpaceId(42),
            })
        );
        assert_eq!(
            repository
                .spaces
                .lock()
                .expect("lock spaces")
                .get(&SpaceId(42))
                .cloned(),
            Some(Space {
                id: SpaceId(42),
                auto_subscribe: false,
            })
        );
    }

    #[tokio::test]
    async fn ensure_personal_space_is_idempotent_for_existing_binding() {
        let repository = Arc::new(InMemorySpaces::default());
        let spaces = Spaces::new(repository.clone(), Arc::new(IncrementingIds::new(7)));

        let first = spaces
            .ensure_personal_space(UserId(3), false)
            .await
            .expect("first ensure");
        let second = spaces
            .ensure_personal_space(UserId(3), true)
            .await
            .expect("second ensure");

        assert_eq!(first.read_data(), second.read_data());
        assert_eq!(second.read_data().id, SpaceId(7));
        assert!(!second.read_data().auto_subscribe);
        assert_eq!(repository.spaces.lock().expect("lock spaces").len(), 1);
        assert_eq!(repository.bindings.lock().expect("lock bindings").len(), 1);
    }
}
