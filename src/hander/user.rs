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
    route::Service,
};

#[utoipa::path(
        post,
        path = "/v1/login",
        responses(
            (status = 200, description = "用户登录", body = JsonResultAuthBody)
        )
    )]
#[axum_macros::debug_handler]
pub async fn login(
    Extension(service): Extension<Service>,
    Json(req): Json<LoginReq>,
) -> ErrorResult<Json<JsonResult<AuthBody>>> {
    let user = service
        .user_db
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

#[utoipa::path(
        get,
        path = "/v1/register/code",
        params(
            RegisterCodeReq,
        ),
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "管理员获取注册码", body = JsonResultRegisterCodeRsp)
        )
    )]
#[axum_macros::debug_handler]
pub async fn register_code(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
    Query(params): Query<RegisterCodeReq>,
) -> ErrorResult<Json<JsonResult<RegisterCodeRsp>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }
    let code = Uuid::new_v4();

    service.user_db.set_register_code(RegisterCode {
        now: Local::now().timestamp(),
        expire: params.expire,
        code: code.to_string(),
        timers: params.timers,
    })?;

    JsonResult::json_ok(Some(RegisterCodeRsp {
        code: code.to_string(),
    }))
}

#[utoipa::path(
        post,
        path = "/v1/register",
        responses(
            (status = 200, description = "注册用户", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn register(
    Extension(service): Extension<Service>,
    Json(req): Json<RegisterReq>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    let user = service
        .user_db
        .get_user_from_username(req.username.as_str())?;
    if user.is_some() {
        return Err(Error::InvalidRequest);
    }

    let register_code = service.user_db.get_register_code(req.code.to_string())?;
    if register_code.is_none() {
        return Err(Error::InvalidRequest);
    }

    let user = UserEntity {
        id: 0,
        username: req.username,
        password: UserEntity::into_sha256_pwd(req.password),
        chatacter: UserCharacter::User,
    };
    service.user_db.create_user(user)?;
    let mut register_code = register_code.unwrap();
    register_code.timers -= 1;
    service.user_db.set_register_code(register_code)?;

    JsonResult::json_ok(None)
}

#[utoipa::path(
        get,
        path = "/v1/users",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "获取所有用户", body = JsonResultVecUserEntity)
        )
    )]
#[axum_macros::debug_handler]
pub async fn users(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<Vec<UserEntity>>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }

    JsonResult::json_ok(service.user_db.get_users()?)
}
