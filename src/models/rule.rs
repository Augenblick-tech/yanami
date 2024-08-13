use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Rule {
    pub cost: usize,
    pub re: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GroupRule {
    pub name: String,
    pub rules: Vec<Rule>,
}
