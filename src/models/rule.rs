use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Rule {
    pub name: String,
    pub cost: usize,
    pub re: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct DelRule {
    pub name: String,
}
