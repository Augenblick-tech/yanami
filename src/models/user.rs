use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    id: i32,
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct UserEntity {
    pub id: i64,
    pub username: String,
    pub password: String,
}
