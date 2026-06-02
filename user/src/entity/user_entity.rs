use domain::shared::error::DomainError;

use crate::entity::cap::UserPasswordGenerator;

#[derive(Debug, Clone)]
pub enum UserRole {
    Admin,
    User,
}

#[derive(Debug, Clone)]
pub struct UserEntity {
    id: u32,
    username: String,
    password: String,
    role: UserRole,
}

impl UserEntity {
    pub(crate) fn new(
        id: u32,
        username: String,
        password: String,
        role: UserRole,
        pwd_cap: &dyn UserPasswordGenerator,
    ) -> Result<Self, DomainError> {
        if id <= 0 {
            return Err(DomainError::InvariantViolation("user id must be not empty"));
        }
        if username.is_empty() && password.is_empty() {
            return Err(DomainError::InvariantViolation(
                "username or password must be not empty",
            ));
        }
        let password = pwd_cap.generator(&password)?;
        Ok(Self {
            id,
            username,
            password,
            role,
        })
    }

    pub fn role(&self) -> &UserRole {
        &self.role
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn verify_password(
        &self,
        password: &str,
        pwd_cap: &dyn UserPasswordGenerator,
    ) -> Result<bool, DomainError> {
        if password.is_empty() {
            return Ok(false);
        }
        pwd_cap.verify(password, &self.password)?;
        Ok(true)
    }

    pub async fn set_password(
        &mut self,
        old_password: &str,
        new_password: &str,
        pwd_cap: &dyn UserPasswordGenerator,
    ) -> Result<(), DomainError> {
        if !pwd_cap.verify(old_password, &self.password)? {
            return Err(DomainError::InvariantViolation("update password failed"));
        }
        if old_password == new_password {
            return Err(DomainError::InvariantViolation(
                "new password must be not same of old",
            ));
        }
        let pwd_hash = pwd_cap.generator(new_password)?;
        self.password = pwd_hash;
        Ok(())
    }
}
