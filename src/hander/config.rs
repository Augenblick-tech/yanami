use axum::{extract::Query, Extension, Json};

use crate::{
    common::{
        auth::{Claims, UserCharacter},
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::config::ServiceConfig,
    route::Service,
};

#[utoipa::path(
        post,
        path = "/v1/config",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "管理员设置系统配置", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn set_config(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
    Query(req): Query<ServiceConfig>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }
    if req.path.is_empty() {
        return Err(Error::InvalidRequest);
    }

    if !req.qbit_url.is_empty() {
        if req.username.is_empty() || req.password.is_empty() {
            return Err(Error::InvalidRequest);
        }
        // TODO:
        // 登录qbit确认账号密码是否正确，正确则记录数据库
    }

    service.path.set_path(&req.path)?;
    JsonResult::json_ok(None)
}

#[utoipa::path(
        get,
        path = "/v1/config",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "管理员获取系统配置", body = JsonResultDownloadPath)
        )
    )]
#[axum_macros::debug_handler]
pub async fn get_config(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<ServiceConfig>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(Some(ServiceConfig {
        path: service.path.get_path()?.unwrap_or("".to_string()),
        qbit_url: "".to_string(),
        username: "".to_string(),
        password: "".to_string(),
    }))
}
