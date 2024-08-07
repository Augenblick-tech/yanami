use axum::{extract::Query, Extension, Json};
use chrono::Local;
use jsonwebtoken::{encode, Header};
use uuid::Uuid;

use crate::{
    common::{
        auth::{Claims, UserCharacter, KEYS},
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::user::{AuthBody, LoginReq, Register, RegisterReq, RegisterRsp, UserEntity},
    route::ServiceRegister,
};

#[axum_macros::debug_handler]
pub async fn login(
    Extension(user_service): Extension<ServiceRegister>,
    Json(req): Json<LoginReq>,
) -> ErrorResult<Json<JsonResult<AuthBody>>> {
    tracing::debug!("login request {:?}", &req);

    let user = user_service
        .provider
        .get_user_from_username(req.username.as_str())?;

    if UserEntity::into_sha256_pwd(req.password) != user.password {
        return Err(Error::InvalidRequest);
    }

    let mut time = Local::now().timestamp();
    time = time + (60 * 60 * 24 * 30);
    let claims = Claims {
        user_id: user.id,
        exp: time as usize,
        character: user.chatacter.into(),
    };
    let token = encode(&Header::default(), &claims, &KEYS.get().unwrap().encoding)
        .map_err(|_| Error::InvalidToken)?;
    JsonResult::json_ok(AuthBody::new(token, time as usize))
}

#[axum_macros::debug_handler]
pub async fn register_code(
    Extension(c): Extension<Claims>,
    Extension(user_service): Extension<ServiceRegister>,
    Query(params): Query<RegisterReq>,
) -> ErrorResult<Json<JsonResult<RegisterRsp>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }

    user_service.provider.set_register_code(Register {
        now: Local::now().timestamp(),
        expire: params.expire,
        code: Uuid::new_v4().to_string(),
        timers: params.timers,
    })?;

    JsonResult::json_ok(RegisterRsp {
        code: "asd".to_string(),
    })
}
