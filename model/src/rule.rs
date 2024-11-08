use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Rule {
    pub name: String,
    pub cost: usize,
    pub re: String,
}

impl From<entity::rule::Model> for Rule {
    fn from(value: entity::rule::Model) -> Self {
        Self {
            name: value.name,
            cost: value.cost as usize,
            re: value.re,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct DelRule {
    pub name: String,
}
