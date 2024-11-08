use std::{borrow::Cow, collections::HashMap};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;
use utoipa::ToSchema;
use validator::{ValidationErrors, ValidationErrorsKind};

use super::result::JsonResult;

pub type ErrorResult<T> = Result<T, Error>;

pub type ConduitErrorMap = HashMap<Cow<'static, str>, Vec<Cow<'static, str>>>;

#[derive(Error, Debug, ToSchema)]
pub enum Error {
    #[error("Invalid token")]
    InvalidToken,
    #[error("Invalid request")]
    InvalidRequest,
    #[error(transparent)]
    AnyhowError(#[from] anyhow::Error),
    #[error(transparent)]
    ValidationError(#[from] ValidationErrors),
}

impl Error {
    pub fn unprocessable_entity(errors: ValidationErrors) -> Response {
        let mut validation_errors = ConduitErrorMap::new();

        // roll through the struct errors at the top level
        for (_, error_kind) in errors.into_errors() {
            // structs may contain validators on themselves, roll through first-depth validators
            if let ValidationErrorsKind::Struct(meta) = error_kind {
                // on structs with validation errors, roll through each of the structs properties to build a list of errors
                for (struct_property, struct_error_kind) in meta.into_errors() {
                    if let ValidationErrorsKind::Field(field_meta) = struct_error_kind {
                        for error in field_meta.into_iter() {
                            validation_errors
                                .entry(Cow::from(struct_property))
                                .or_default()
                                .push(error.message.unwrap_or_else(|| {
                                    // required validators contain None for their message, assume a default response
                                    Cow::from(format!("{} is required", struct_property))
                                }));
                        }
                    }
                }
            }
        }

        let body = Json(json!({
            "error": validation_errors,
        }));

        (StatusCode::UNPROCESSABLE_ENTITY, body).into_response()
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if let Self::ValidationError(e) = self {
            return Self::unprocessable_entity(e);
        }

        // let (status, error_message) = match self {
        // Error::InvalidToken => (StatusCode::OK, "invalid token".to_string()),
        // Error::InvalidRequest => (StatusCode::OK, "invalid request".to_string()),
        // _ => (
        //     StatusCode::OK,
        //     self.to_string(),
        // String::from("INTERNAL_SERVER_ERROR"),
        // ),
        // };
        let status = StatusCode::OK;
        let error_message = self.to_string();

        let body = JsonResult::<String>::json_err(error_message).unwrap();
        (status, body).into_response()
    }
}
