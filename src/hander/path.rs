use axum::{extract::Query, Extension, Json};

use crate::{
    common::{
        auth::{Claims, UserCharacter},
        errors::{Error, ErrorResult},
        result::JsonResult,
    },
    models::path::DownloadPath,
    route::Service,
};

#[utoipa::path(
        post,
        path = "/v1/path",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "管理员设置下载路径", body = JsonResulti32)
        )
    )]
#[axum_macros::debug_handler]
pub async fn set_path(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
    Query(req): Query<DownloadPath>,
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
    service.path.set_path(&req.path)?;
    JsonResult::json_ok(None)
}

#[utoipa::path(
        get,
        path = "/v1/path",
        security(("api_key" = ["Authorization"])),
        responses(
            (status = 200, description = "管理员获取下载路径", body = JsonResultDownloadPath)
        )
    )]
#[axum_macros::debug_handler]
pub async fn get_path(
    Extension(c): Extension<Claims>,
    Extension(service): Extension<Service>,
) -> ErrorResult<Json<JsonResult<DownloadPath>>> {
    if !match UserCharacter::from(c.character.as_str()) {
        UserCharacter::Admin => true,
        _ => false,
    } {
        return Err(Error::InvalidRequest);
    }
    JsonResult::json_ok(Some(DownloadPath {
        path: service.path.get_path()?.unwrap_or("".to_string()),
    }))
}
