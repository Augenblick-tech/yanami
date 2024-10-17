use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};

use crate::common::auth::UserCharacter;

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    id: i32,
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SetUserPassword {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthBody {
    access_token: String,
    token_type: String,
    expired: usize,
}

impl AuthBody {
    pub fn new(access_token: String, expired: usize) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
            expired,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserEntity {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub chatacter: UserCharacter,
}

impl UserEntity {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        Ok(serde_json::from_slice::<UserEntity>(bytes)?)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, anyhow::Error> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn into_sha256_pwd(password: String) -> String {
        format!(
            "{:x}",
            Sha256::digest(format!("yanami66{}", password).into_bytes())
        )
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct RegisterCodeReq {
    pub timers: usize,
    pub expire: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterCodeRsp {
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterCode {
    pub timers: usize,
    pub expire: i64,
    pub now: i64,
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterReq {
    pub code: String,
    pub username: String,
    pub password: String,
}
