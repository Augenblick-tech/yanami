use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use domain::{
    shared::biz::BizContext,
    shared::error::DomainError,
    user::{PasswordHash, RegistrationCodeValue, UserId, UserRole},
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use domain::user::{AccessToken, AccessTokenIssuer};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use user::gateway::{EpochClock, PasswordService, RegistrationCodeGenerator, UserIdGenerator};
use uuid::Uuid;

use crate::db::SqliteDb;

/// 兼容旧系统口径的密码摘要实现。
pub struct LegacySha256PasswordService;

#[async_trait]
impl PasswordService for LegacySha256PasswordService {
    async fn hash_password(&self, plaintext: &str) -> Result<PasswordHash, DomainError> {
        Ok(PasswordHash(legacy_hash_password(plaintext)))
    }

    async fn verify_password(
        &self,
        password_hash: &PasswordHash,
        plaintext: &str,
    ) -> Result<bool, DomainError> {
        Ok(password_hash.0 == legacy_hash_password(plaintext))
    }
}

/// JWT 访问令牌签发实现。
pub struct JwtAccessTokenIssuer {
    encoding_key: EncodingKey,
    expires_in_seconds: i64,
}

impl JwtAccessTokenIssuer {
    pub fn new(application_key: &str, expires_in_seconds: i64) -> Result<Self, DomainError> {
        if application_key.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "application key cannot be empty",
            ));
        }
        if expires_in_seconds <= 0 {
            return Err(DomainError::InvariantViolation(
                "token ttl must be positive",
            ));
        }

        Ok(Self {
            encoding_key: EncodingKey::from_secret(application_key.as_bytes()),
            expires_in_seconds,
        })
    }
}

#[async_trait]
impl AccessTokenIssuer for JwtAccessTokenIssuer {
    async fn issue_access_token(
        &self,
        user_id: UserId,
        role: UserRole,
    ) -> Result<AccessToken, DomainError> {
        let expires_at = Utc::now().timestamp() + self.expires_in_seconds;
        let access_token = encode(
            &Header::default(),
            &AccessTokenClaims {
                user_id: user_id.0,
                exp: expires_at as usize,
                character: match role {
                    UserRole::Admin => "admin".to_string(),
                    UserRole::User => "user".to_string(),
                },
            },
            &self.encoding_key,
        )
        .map_err(|error| DomainError::external("access token issue failed", error))?;

        Ok(AccessToken {
            access_token,
            token_type: "Bearer".to_string(),
            expires_at,
        })
    }
}

/// 系统时钟实现。
pub struct SystemEpochClock;

impl EpochClock for SystemEpochClock {
    fn now_epoch_seconds(&self) -> i64 {
        Utc::now().timestamp()
    }
}

/// UUID 注册码生成实现。
pub struct UuidRegistrationCodeGenerator;

#[async_trait]
impl RegistrationCodeGenerator for UuidRegistrationCodeGenerator {
    async fn generate_registration_code(&self) -> Result<RegistrationCodeValue, DomainError> {
        Ok(RegistrationCodeValue(Uuid::new_v4().to_string()))
    }
}

/// 基于 SQLite 当前最大用户 ID 的单实例分配器。
pub struct SqliteUserIdGenerator {
    db: Arc<SqliteDb>,
    next_id: Mutex<Option<i64>>,
}

impl SqliteUserIdGenerator {
    pub fn new(db: Arc<SqliteDb>) -> Self {
        Self {
            db,
            next_id: Mutex::new(None),
        }
    }
}

#[async_trait]
impl UserIdGenerator for SqliteUserIdGenerator {
    async fn next_user_id(&self) -> Result<UserId, DomainError> {
        let mut next_id = self.next_id.lock().await;
        let current = match *next_id {
            Some(current) => {
                *next_id = Some(current + 1);
                current
            }
            None => {
                let initialized = self.db.next_user_account_id_value().await?;
                *next_id = Some(initialized + 1);
                initialized
            }
        };
        Ok(UserId(current))
    }

    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn UserIdGenerator>, DomainError> {
        Ok(Arc::new(crate::db::SqliteBizUserIds::new(
            self.db.bind_biz_provider(biz)?,
        )))
    }
}

#[derive(Debug, Clone, Serialize)]
struct AccessTokenClaims {
    user_id: i64,
    exp: usize,
    character: String,
}

pub fn legacy_hash_password(plaintext: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("yanami66{plaintext}").into_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn legacy_password_service_matches_historic_hash() {
        let service = LegacySha256PasswordService;

        let hash = service.hash_password("123456").await.expect("hash");
        let matched = service
            .verify_password(&hash, "123456")
            .await
            .expect("verify");

        assert!(matched);
    }

    #[tokio::test]
    async fn sqlite_user_id_generator_allocates_monotonically() {
        let db = Arc::new(
            SqliteDb::new("sqlite::memory:", "test-key")
                .await
                .expect("db"),
        );
        let generator = SqliteUserIdGenerator::new(db);

        let first = generator.next_user_id().await.expect("first");
        let second = generator.next_user_id().await.expect("second");

        assert_eq!(first, UserId(10000));
        assert_eq!(second, UserId(10001));
    }
}
