use std::{sync::Arc, time::Duration};

use dashmap::DashMap;
use regex::Regex;
use reqwest::Client;
use sqlx::{Pool, Sqlite};

use crate::Boss;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub token: String,
    pub expire: Duration,
}

#[derive(Clone)]
pub struct AppContext {
    pub pool: Pool<Sqlite>,
    pub regex_cache: Arc<DashMap<String, Regex>>,
    pub http_client: Client,
    pub auth_config: AuthConfig,
}

impl AppContext {
    pub fn new(
        pool: Pool<Sqlite>,
        regex_cache: DashMap<String, Regex>,
        http_client: Client,
        auth_config: AuthConfig,
    ) -> Self {
        Self {
            pool,
            regex_cache: Arc::new(regex_cache),
            http_client,
            auth_config,
        }
    }
}

impl Boss for AppContext {
    type Ctx = AppContext;

    fn context(&self) -> &Self::Ctx {
        self
    }
}
