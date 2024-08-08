use crate::common::errors::Error;
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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
