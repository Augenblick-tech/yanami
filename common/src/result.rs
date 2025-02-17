use model::{anime::AnimeStatus, config::ServiceConfig, rss::{AnimeRssRecord, RSS}, rule::Rule, user::{AuthBody, RegisterCodeRsp, UserEntity}};
use anna::anime::tracker::AnimeInfo;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::errors::Error;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[aliases(
    JsonResultAuthBody = JsonResult<AuthBody>, 
    JsonResultVecUserEntity = JsonResult<Vec<UserEntity>>, 
    JsonResulti32 = JsonResult<i32>, 
    JsonResultRegisterCodeRsp = JsonResult<RegisterCodeRsp>,
    JsonResultVecRSS = JsonResult<Vec<RSS>>,
    JsonResultRSS = JsonResult<RSS>,
    JsonResultVecRule = JsonResult<Vec<Rule>>,
    JsonResultDownloadPath = JsonResult<ServiceConfig>,
    JsonResultVecAnimeInfo = JsonResult<Vec<AnimeInfo>>,
    JsonResultVecAnimeStatus = JsonResult<Vec<AnimeStatus>>,
    JsonResultAnimeStatus = JsonResult<AnimeStatus>,
    JsonResultVecAnimeRssRecord = JsonResult<Vec<AnimeRssRecord>>,
)]
pub struct JsonResult<T> {
    code: i32,
    data: Option<T>,
    msg: String,
}

impl<T> JsonResult<T> {
    fn new(code: i32, data: Option<T>, msg: String) -> Self {
        JsonResult { code, data, msg }
    }

    fn ok(data: Option<T>) -> Self {
        Self::new(200, data, "".to_string())
    }

    fn err(msg: String) -> Self {
        Self::new(500, None, msg)
    }

    pub fn json(data: T) -> Json<JsonResult<T>> {
        Json(JsonResult {
            code: 200,
            data: Some(data),
            msg: "".to_string(),
        })
    }

    pub fn json_err(msg: String) -> Result<Json<JsonResult<T>>, Error> {
        Ok(Json(JsonResult::<T>::err(msg)))
    }

    pub fn json_ok(data: Option<T>) -> Result<Json<JsonResult<T>>, Error> {
        Ok(Json(JsonResult::ok(data)))
    }
}
