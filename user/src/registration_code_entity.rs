use domain::{
    shared::error::DomainError,
    user::{RegistrationCode, RegistrationCodeValue},
};
use std::fmt;

use crate::gateway::EpochClock;

#[derive(Clone)]
pub struct RegistrationCodeEntity<'a> {
    snapshot: RegistrationCode,
    clock: &'a dyn EpochClock,
}

impl fmt::Debug for RegistrationCodeEntity<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegistrationCodeEntity")
            .field("code", &self.snapshot.code)
            .field("remaining_uses", &self.snapshot.remaining_uses)
            .finish()
    }
}

impl<'a> RegistrationCodeEntity<'a> {
    /// 基于注册码快照构造注册码聚合根。
    pub fn new(snapshot: RegistrationCode, clock: &'a dyn EpochClock) -> Self {
        Self { snapshot, clock }
    }

    pub fn snapshot(&self) -> &RegistrationCode {
        &self.snapshot
    }

    pub fn into_snapshot(self) -> RegistrationCode {
        self.snapshot
    }

    pub fn code(&self) -> &RegistrationCodeValue {
        &self.snapshot.code
    }

    pub fn assert_usable(&self) -> Result<(), DomainError> {
        if self.snapshot.code.0.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "registration code cannot be empty",
            ));
        }
        let now = self.clock.now_epoch_seconds();
        if self.snapshot.issued_at + self.snapshot.valid_for_seconds <= now {
            return Err(DomainError::InvariantViolation(
                "registration code is expired",
            ));
        }
        if self.snapshot.remaining_uses == 0 {
            return Err(DomainError::InvariantViolation(
                "registration code is exhausted",
            ));
        }
        Ok(())
    }

    pub fn consume_once(&mut self) -> Result<(), DomainError> {
        self.assert_usable()?;
        self.snapshot.remaining_uses -= 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedClock(i64);

    impl EpochClock for FixedClock {
        fn now_epoch_seconds(&self) -> i64 {
            self.0
        }
    }

    fn sample_code() -> RegistrationCode {
        RegistrationCode {
            code: RegistrationCodeValue("invite".to_string()),
            issued_at: 100,
            valid_for_seconds: 60,
            remaining_uses: 2,
        }
    }

    #[test]
    fn assert_usable_rejects_empty_code() {
        let entity = RegistrationCodeEntity::new(
            RegistrationCode {
                code: RegistrationCodeValue("   ".to_string()),
                ..sample_code()
            },
            &FixedClock(120),
        );

        let error = entity.assert_usable().expect_err("empty code must fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: registration code cannot be empty"
        );
    }

    #[test]
    fn assert_usable_rejects_expired_and_exhausted_code() {
        let expired = RegistrationCodeEntity::new(sample_code(), &FixedClock(160));
        let exhausted = RegistrationCodeEntity::new(
            RegistrationCode {
                remaining_uses: 0,
                ..sample_code()
            },
            &FixedClock(120),
        );

        assert_eq!(
            expired
                .assert_usable()
                .expect_err("expired code must fail")
                .to_string(),
            "domain invariant violation: registration code is expired"
        );
        assert_eq!(
            exhausted
                .assert_usable()
                .expect_err("exhausted code must fail")
                .to_string(),
            "domain invariant violation: registration code is exhausted"
        );
    }

    #[test]
    fn consume_once_updates_snapshot_until_exhausted() {
        let mut entity = RegistrationCodeEntity::new(sample_code(), &FixedClock(120));

        entity.consume_once().expect("first use");
        entity.consume_once().expect("second use");
        let error = entity.consume_once().expect_err("third use must fail");

        assert_eq!(entity.snapshot().remaining_uses, 0);
        assert_eq!(
            error.to_string(),
            "domain invariant violation: registration code is exhausted"
        );
    }
}
