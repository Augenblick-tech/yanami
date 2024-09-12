use anna::qbit::qbitorrent::Qbit;
use axum::{Extension, Json};

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
    Json(req): Json<ServiceConfig>,
) -> ErrorResult<Json<JsonResult<i32>>> {
    if !matches!(
        UserCharacter::from(c.character.as_str()),
        UserCharacter::Admin
    ) {
        return Err(Error::InvalidRequest);
    }
    if req.path.is_empty() {
        return Err(Error::InvalidRequest);
    }

    if let Some(config) = req.qbit_config {
        if config.url.is_empty() || config.username.is_empty() || config.password.is_empty() {
            return Err(Error::InvalidRequest);
        }
        // 登录qbit确认账号密码是否正确，正确则记录数据库
        Qbit::new(
            config.url.clone(),
            config.username.clone(),
            config.password.clone(),
        )
        .login()
        .await?;
        service
            .config
            .set_qbit(&config.url, &config.username, &config.password)?;
    }
    service.config.set_path(&req.path)?;
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
    if !matches!(
        UserCharacter::from(c.character.as_str()),
        UserCharacter::Admin
    ) {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(Some(ServiceConfig {
        path: service.config.get_path()?.unwrap_or("".to_string()),
        qbit_config: service.config.get_qbit()?,
    }))
}
