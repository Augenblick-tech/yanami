use std::sync::Arc;

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, Request},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tracing::warn;

use crate::http::{error::ApiError, state::AppState};

/// 认证成功后的当前用户。
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedUser {
    /// 当前用户标识。
    pub user_id: domain::user::UserId,
}

/// JWT 解码器。
#[derive(Clone)]
pub struct JwtDecoder {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtDecoder {
    /// 使用应用密钥创建 JWT 解码器。
    pub fn new(application_key: &str) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        Self {
            decoding_key: DecodingKey::from_secret(application_key.as_bytes()),
            validation,
        }
    }

    /// 解析并校验一个访问令牌。
    pub fn decode_token(&self, token: &str) -> Result<AuthenticatedUser, ApiError> {
        let claims = decode::<AccessTokenClaims>(token, &self.decoding_key, &self.validation)
            .map_err(|error| {
                warn!(error = ?error, "failed to decode access token");
                ApiError::unauthorized("invalid access token")
            })?
            .claims;
        match claims.character.as_str() {
            "admin" => domain::user::UserRole::Admin,
            "user" => domain::user::UserRole::User,
            _ => {
                warn!(role = claims.character, "invalid access token role");
                return Err(ApiError::unauthorized("invalid access token"));
            }
        };
        Ok(AuthenticatedUser {
            user_id: domain::user::UserId(claims.user_id),
        })
    }
}

/// Bearer 认证中间件。
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("invalid authorization scheme"))?;
    let user = state.auth.decode_token(token)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

#[derive(Debug, Clone, Deserialize)]
struct AccessTokenClaims {
    user_id: i64,
    character: String,
}
