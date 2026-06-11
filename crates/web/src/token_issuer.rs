use chrono::Utc;
use common::shared::error::Error;
use jsonwebtoken::{EncodingKey, Header, encode};
use user::entity::model::UserRole;

use crate::model::{AccessToken, AccessTokenClaims};

/// JWT 访问令牌签发实现。
pub struct JwtAccessTokenIssuer {
    encoding_key: EncodingKey,
    expires_in_seconds: i64,
}

impl JwtAccessTokenIssuer {
    pub fn new(application_key: &str, expires_in_seconds: i64) -> Result<Self, Error> {
        if application_key.trim().is_empty() {
            return Err(Error::invariant("application key cannot be empty"));
        }
        if expires_in_seconds <= 0 {
            return Err(Error::invariant("token ttl must be positive"));
        }

        Ok(Self {
            encoding_key: EncodingKey::from_secret(application_key.as_bytes()),
            expires_in_seconds,
        })
    }
}

impl JwtAccessTokenIssuer {
    pub async fn issue_access_token(
        &self,
        user_id: i64,
        role: UserRole,
    ) -> Result<AccessToken, Error> {
        let expires_at = Utc::now().timestamp() + self.expires_in_seconds;
        let access_token = encode(
            &Header::default(),
            &AccessTokenClaims {
                user_id,
                exp: expires_at as usize,
                character: role,
            },
            &self.encoding_key,
        )
        .map_err(|error| Error::external("access token issue failed", error))?;

        Ok(AccessToken {
            access_token,
            token_type: "Bearer".to_string(),
            expires_at,
        })
    }
}
