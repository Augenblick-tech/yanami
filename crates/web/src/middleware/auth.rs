use std::sync::Arc;

use axum::{
    Extension,
    extract::State,
    http::{Request, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use tracing::warn;
use user::entity::model::UserRole;

use crate::{app_ctx::AppContext, error::ApiError, model::AccessTokenClaims};

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
    pub fn decode_token(&self, token: &str) -> Result<AccessTokenClaims, ApiError> {
        let claims = decode::<AccessTokenClaims>(token, &self.decoding_key, &self.validation)
            .map_err(|error| {
                warn!(error = ?error, "failed to decode access token");
                ApiError::unauthorized("invalid access token")
            })?
            .claims;
        Ok(claims)
    }
}

/// Bearer 认证中间件。
pub async fn require_auth(
    State(ctx): State<Arc<AppContext>>,
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
    let user = ctx.caps.jwt_decoder.decode_token(token)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

pub async fn require_admin(
    Extension(user): Extension<AccessTokenClaims>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if user.character != UserRole::Admin {
        Err(ApiError::forbidden("require admin privilege"))
    } else {
        Ok(next.run(request).await)
    }
}
