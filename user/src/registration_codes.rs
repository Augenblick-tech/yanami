use std::sync::Arc;

use domain::{
    shared::biz::BizContext,
    shared::error::DomainError,
    user::{RegistrationCode, RegistrationCodeRepository, RegistrationCodeValue},
};

use crate::{
    gateway::{EpochClock, RegistrationCodeGenerator},
    registration_code_entity::RegistrationCodeEntity,
};

#[derive(Clone)]
pub struct RegistrationCodes {
    repository: Arc<dyn RegistrationCodeRepository>,
    generator: Arc<dyn RegistrationCodeGenerator>,
    clock: Arc<dyn EpochClock>,
}

impl RegistrationCodes {
    /// 构造注册码聚合根集合入口。
    pub fn new(
        repository: Arc<dyn RegistrationCodeRepository>,
        generator: Arc<dyn RegistrationCodeGenerator>,
        clock: Arc<dyn EpochClock>,
    ) -> Self {
        Self {
            repository,
            generator,
            clock,
        }
    }

    pub fn with_biz(&self, biz: &BizContext) -> Result<Self, DomainError> {
        Ok(Self {
            repository: self.repository.with_biz(biz)?,
            generator: self.generator.clone(),
            clock: self.clock.clone(),
        })
    }

    pub async fn load(&self, code: &str) -> Result<Option<RegistrationCodeEntity>, DomainError> {
        let value = RegistrationCodeValue(code.trim().to_string());
        if value.0.is_empty() {
            return Err(DomainError::InvariantViolation(
                "registration code cannot be empty",
            ));
        }
        Ok(self
            .repository
            .find_registration_code(&value)
            .await?
            .map(RegistrationCodeEntity::new))
    }

    pub async fn create(
        &self,
        valid_for_seconds: i64,
        remaining_uses: u32,
    ) -> Result<RegistrationCodeEntity, DomainError> {
        if valid_for_seconds <= 0 {
            return Err(DomainError::InvariantViolation(
                "registration code ttl must be positive",
            ));
        }
        if remaining_uses == 0 {
            return Err(DomainError::InvariantViolation(
                "registration code remaining uses must be positive",
            ));
        }

        let snapshot = RegistrationCode {
            code: self.generator.generate_registration_code().await?,
            issued_at: self.clock.now_epoch_seconds(),
            valid_for_seconds,
            remaining_uses,
        };
        self.repository.save_registration_code(&snapshot).await?;
        Ok(RegistrationCodeEntity::new(snapshot))
    }

    pub async fn save(&self, entity: &RegistrationCodeEntity) -> Result<(), DomainError> {
        self.repository
            .save_registration_code(entity.snapshot())
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct InMemoryRepository {
        items: Mutex<HashMap<String, RegistrationCode>>,
    }

    #[async_trait]
    impl RegistrationCodeRepository for InMemoryRepository {
        async fn find_registration_code(
            &self,
            code: &RegistrationCodeValue,
        ) -> Result<Option<RegistrationCode>, DomainError> {
            Ok(self.items.lock().expect("items").get(&code.0).cloned())
        }

        async fn save_registration_code(
            &self,
            registration_code: &RegistrationCode,
        ) -> Result<(), DomainError> {
            self.items
                .lock()
                .expect("items")
                .insert(registration_code.code.0.clone(), registration_code.clone());
            Ok(())
        }
    }

    struct FixedGenerator(&'static str);

    #[async_trait]
    impl RegistrationCodeGenerator for FixedGenerator {
        async fn generate_registration_code(&self) -> Result<RegistrationCodeValue, DomainError> {
            Ok(RegistrationCodeValue(self.0.to_string()))
        }
    }

    struct FixedClock(i64);

    impl EpochClock for FixedClock {
        fn now_epoch_seconds(&self) -> i64 {
            self.0
        }
    }

    #[tokio::test]
    async fn load_rejects_empty_code_and_returns_none_for_missing() {
        let codes = RegistrationCodes::new(
            Arc::new(InMemoryRepository::default()),
            Arc::new(FixedGenerator("invite")),
            Arc::new(FixedClock(100)),
        );

        let error = codes.load("  ").await.expect_err("empty code must fail");
        let missing = codes.load("missing").await.expect("missing");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: registration code cannot be empty"
        );
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn issue_validates_input_and_persists_code() {
        let repository = Arc::new(InMemoryRepository::default());
        let codes = RegistrationCodes::new(
            repository.clone(),
            Arc::new(FixedGenerator("invite")),
            Arc::new(FixedClock(123)),
        );

        let ttl_error = codes.create(0, 1).await.expect_err("ttl");
        let uses_error = codes.create(10, 0).await.expect_err("uses");
        let code = codes.create(60, 2).await.expect("issue");

        assert_eq!(
            ttl_error.to_string(),
            "domain invariant violation: registration code ttl must be positive"
        );
        assert_eq!(
            uses_error.to_string(),
            "domain invariant violation: registration code remaining uses must be positive"
        );
        assert_eq!(code.snapshot().issued_at, 123);
        assert_eq!(
            repository
                .find_registration_code(&RegistrationCodeValue("invite".to_string()))
                .await
                .expect("find")
                .expect("exists")
                .remaining_uses,
            2
        );
    }

    #[tokio::test]
    async fn consume_once_persists_updated_snapshot() {
        let repository = Arc::new(InMemoryRepository::default());
        let codes = RegistrationCodes::new(
            repository.clone(),
            Arc::new(FixedGenerator("invite")),
            Arc::new(FixedClock(100)),
        );
        let missing = codes.load("invite").await.expect("missing first");
        assert!(missing.is_none());
        codes.create(60, 2).await.expect("issue");
        let mut updated = codes
            .load("invite")
            .await
            .expect("load")
            .expect("issued code");
        updated.consume_once(100).expect("consume");
        codes.save(&updated).await.expect("save consumed code");
        assert_eq!(updated.snapshot().remaining_uses, 1);

        assert_eq!(
            repository
                .find_registration_code(&RegistrationCodeValue("invite".to_string()))
                .await
                .expect("find")
                .expect("exists")
                .remaining_uses,
            1
        );
    }
}
