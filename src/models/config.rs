use anna::qbit::qbit::QbitConfig;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct ServiceConfig {
    pub path: String,
    pub qbit_config: Option<QbitConfig>,
}
