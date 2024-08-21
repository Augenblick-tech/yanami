use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct ServiceConfig {
    pub path: String,
    pub qbit_url: String,
    pub username: String,
    pub password: String,
}
