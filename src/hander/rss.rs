use axum::{Extension, Json};

use crate::{
    common::{auth::Claims, errors::ErrorResult, result::JsonResult},
    models::rss::RSS,
    route::ServiceRegister,
};

#[axum_macros::debug_handler]
pub async fn rss_list(
    Extension(user_service): Extension<ServiceRegister>,
) -> ErrorResult<Json<JsonResult<Vec<RSS>>>> {
    JsonResult::json_ok(user_service.provider.get_all_rss()?)
}

// #[axum_macros::debug_handler]
// pub async fn set_rss(
//     Extension(user_service): Extension<ServiceRegister>,
// ) -> ErrorResult<Json<JsonResult<String>>> {
//     JsonResult::json_ok(user_service.provider.get_all_rss()?)
// }
