use common::shared::error::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum UserRole {
    Admin,
    User,
}

impl TryFrom<u8> for UserRole {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(UserRole::Admin),
            2 => Ok(UserRole::User),
            _ => Err(Error::conflict(format!("unknown user role {}", value))),
        }
    }
}

impl From<UserRole> for u8 {
    fn from(value: UserRole) -> Self {
        match value {
            UserRole::Admin => 1,
            UserRole::User => 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Hash, Eq, PartialEq)]
pub enum DownloaderConfig {
    Qbit(DownloadConfig<QbitConfig>),
}

impl DownloaderConfig {
    pub fn is_active(&self) -> bool {
        match self {
            DownloaderConfig::Qbit(download_config) => download_config.active,
        }
    }

    pub fn base_path(&self) -> &str {
        match self {
            DownloaderConfig::Qbit(download_config) => &download_config.base_path,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            DownloaderConfig::Qbit(download_config) => &download_config.name,
        }
    }

    pub fn set_active(&mut self, active: bool) {
        match self {
            DownloaderConfig::Qbit(download_config) => download_config.active = active,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Hash, Eq, PartialEq)]
pub struct DownloadConfig<T> {
    pub name: String,
    pub active: bool,
    pub base_path: String,
    pub config: T,
}

#[derive(Debug, Clone, Deserialize, Serialize, Hash, Eq, PartialEq)]
pub struct QbitConfig {
    pub username: String,
    pub password: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct UserBaseData {
    pub id: i64,
    pub username: String,
    pub password: String,
    pub role: UserRole,
    pub space_id: i64,
    pub auto_sub: bool,
    pub download_config: Vec<DownloaderConfig>,
}

#[derive(Debug, Clone)]
pub struct UserProps {
    pub data: UserBaseData,
}
