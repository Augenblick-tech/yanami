use crate::entity::cap::CryptoProvider;
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
    Default(DownloadConfig<DefaultDownloaderConfig>),
}

impl DownloaderConfig {
    pub fn is_active(&self) -> bool {
        match self {
            DownloaderConfig::Qbit(download_config) => download_config.active,
            DownloaderConfig::Default(download_config) => download_config.active,
        }
    }

    pub fn base_path(&self) -> &str {
        match self {
            DownloaderConfig::Qbit(download_config) => &download_config.base_path,
            DownloaderConfig::Default(download_config) => &download_config.base_path,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            DownloaderConfig::Qbit(download_config) => &download_config.name,
            DownloaderConfig::Default(download_config) => &download_config.name,
        }
    }

    pub fn set_active(&mut self, active: bool) {
        match self {
            DownloaderConfig::Qbit(download_config) => download_config.active = active,
            DownloaderConfig::Default(download_config) => download_config.active = active,
        }
    }

    pub fn sanitized(self) -> Self {
        match self {
            DownloaderConfig::Qbit(mut c) => {
                c.config.password = "************".to_string();
                DownloaderConfig::Qbit(c)
            }
            DownloaderConfig::Default(c) => DownloaderConfig::Default(c),
        }
    }

    pub fn encrypt_secrets(&mut self, crypto_provider: &dyn CryptoProvider) -> Result<(), Error> {
        match self {
            DownloaderConfig::Qbit(qbit) => {
                let cipher = crypto_provider
                    .encrypt(&qbit.config.password)
                    .map_err(|e| Error::external("encrypt password failed", e))?;
                qbit.config.password = cipher;
            }
            DownloaderConfig::Default(_) => {}
        }
        Ok(())
    }

    pub fn decrypt_secrets(&mut self, crypto_provider: &dyn CryptoProvider) -> Result<(), Error> {
        match self {
            DownloaderConfig::Qbit(qbit) => {
                let plain = crypto_provider
                    .decrypt(&qbit.config.password)
                    .map_err(|e| Error::external("decrypt password failed", e))?;
                qbit.config.password = plain;
            }
            DownloaderConfig::Default(_) => {}
        }
        Ok(())
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DefaultDownloaderConfig {
    /// 分钟
    pub max_seed_time: Option<u64>,
    pub max_seed_ratio: Option<f64>,
    /// 单位: KB/s
    pub max_upload_speed: Option<u64>,
}

impl Eq for DefaultDownloaderConfig {}

impl std::hash::Hash for DefaultDownloaderConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.max_seed_time.hash(state);
        self.max_upload_speed.hash(state);
        if let Some(ratio) = self.max_seed_ratio {
            state.write_u64(ratio.to_bits());
        } else {
            state.write_u8(0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    Downloading,
    Paused,
    Completed,
    Error(String),
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Downloading => write!(f, "downloading"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Error(e) => write!(f, "error: {}", e),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub hash: [u8; 20],
    pub name: String,
    pub state: DownloadState,
    pub progress: f64,
    /// 单位: 字节 (Bytes)
    pub total_size: u64,
    /// 单位: 字节/秒 (B/s)
    pub download_speed: u64,
    pub is_seeding: bool,
    /// 单位: 字节/秒 (B/s)
    pub upload_speed: u64,
    pub seed_ratio: f64,
    /// 单位: 秒 (s)
    pub seed_duration: Option<u64>,
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
