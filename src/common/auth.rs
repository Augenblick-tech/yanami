use std::fmt::Display;

use async_trait::async_trait;
use axum::{extract::FromRequestParts, http::request::Parts, RequestPartsExt};
use axum_extra::TypedHeader;
use headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, DecodingKey, EncodingKey, Validation};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::errors::Error;
// use crate::CONFIG;

// pub static KEYS: Lazy<Keys> = Lazy::new(|| {
//     let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
//     Keys::new(secret.as_bytes())
// });

pub static KEYS: OnceCell<Keys> = OnceCell::new();

pub fn init(key: String) {
    let r = KEYS.set(Keys::new(key.as_bytes()));
    if let Err(_) = r {
        tracing::error!("set jwt secret err");
    }
}

pub struct Keys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

impl Keys {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct Claims {
    pub user_id: i64,
    pub exp: usize,
    pub character: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub enum UserCharacter {
    Admin,
    User,
    None,
}

impl From<&str> for UserCharacter {
    fn from(value: &str) -> Self {
        match value {
            "admin" => UserCharacter::Admin,
            "user" => UserCharacter::User,
            _ => UserCharacter::None,
        }
    }
}

impl Into<String> for UserCharacter {
    fn into(self) -> String {
        match self {
            UserCharacter::Admin => String::from("admin"),
            UserCharacter::User => String::from("user"),
            UserCharacter::None => String::from(""),
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| Error::InvalidToken)?;
        // Decode the user data
        let token_data = decode::<Claims>(
            bearer.token(),
            &KEYS.get().unwrap().decoding,
            &Validation::default(),
        )
        .map_err(|_| Error::InvalidToken)?;

        Ok(token_data.claims)
    }
}

impl Display for Claims {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UserID: {}\nExp: {}", self.user_id, self.exp)
    }
}
