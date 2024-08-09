use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RSS {
    pub url: String,
    pub title: String,
}
