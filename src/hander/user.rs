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
    models::user::{
        AuthBody, LoginReq, RegisterCode, RegisterCodeReq, RegisterCodeRsp, RegisterReq, UserEntity,
    },
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
    if user.is_none() {
        return Err(Error::InvalidRequest);
    }
    let user = user.unwrap();

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
    JsonResult::json_ok(Some(AuthBody::new(token, time as usize)))
}

#[axum_macros::debug_handler]
pub async fn register_code(
    Extension(c): Extension<Claims>,
    Extension(user_service): Extension<ServiceRegister>,
    Query(params): Query<RegisterCodeReq>,
) -> ErrorResult<Json<JsonResult<RegisterCodeRsp>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }
    let code = Uuid::new_v4();

    user_service.provider.set_register_code(RegisterCode {
        now: Local::now().timestamp(),
        expire: params.expire,
        code: code.to_string(),
        timers: params.timers,
    })?;

    JsonResult::json_ok(Some(RegisterCodeRsp {
        code: code.to_string(),
    }))
}

#[axum_macros::debug_handler]
pub async fn register(
    Extension(user_service): Extension<ServiceRegister>,
    Json(req): Json<RegisterReq>,
) -> ErrorResult<Json<JsonResult<()>>> {
    let user = user_service
        .provider
        .get_user_from_username(req.username.as_str())?;
    if user.is_some() {
        return Err(Error::InvalidRequest);
    }

    let register_code = user_service
        .provider
        .get_register_code(req.code.to_string())?;
    if register_code.is_none() {
        return Err(Error::InvalidRequest);
    }

    let user = UserEntity {
        id: 0,
        username: req.username,
        password: UserEntity::into_sha256_pwd(req.password),
        chatacter: UserCharacter::User,
    };
    user_service.provider.create_user(user)?;
    let mut register_code = register_code.unwrap();
    register_code.timers -= 1;
    user_service.provider.set_register_code(register_code)?;

    JsonResult::json_ok(None)
}

#[axum_macros::debug_handler]
pub async fn users(
    Extension(c): Extension<Claims>,
    Extension(user_service): Extension<ServiceRegister>,
) -> ErrorResult<Json<JsonResult<Vec<UserEntity>>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }

    JsonResult::json_ok(user_service.provider.get_users()?)
}
