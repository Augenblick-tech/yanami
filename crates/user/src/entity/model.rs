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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::crypto::AesCryptoProvider;

    /// 测试专用固定密钥，真实部署由运维注入。
    const TEST_KEY: &str = "yanami-unit-test-fixed-secret";

    fn qbit_config(name: &str, active: bool, base_path: &str, password: &str) -> DownloaderConfig {
        DownloaderConfig::Qbit(DownloadConfig {
            name: name.to_string(),
            active,
            base_path: base_path.to_string(),
            config: QbitConfig {
                username: "admin".to_string(),
                password: password.to_string(),
                url: "http://127.0.0.1:8080/".to_string(),
            },
        })
    }

    fn default_config(name: &str, active: bool, base_path: &str) -> DownloaderConfig {
        DownloaderConfig::Default(DownloadConfig {
            name: name.to_string(),
            active,
            base_path: base_path.to_string(),
            config: DefaultDownloaderConfig {
                max_seed_time: Some(30),
                max_seed_ratio: Some(1.0),
                max_upload_speed: Some(2048),
            },
        })
    }

    fn password_of(config: &DownloaderConfig) -> &str {
        match config {
            DownloaderConfig::Qbit(c) => &c.config.password,
            DownloaderConfig::Default(_) => panic!("test expects qbit config"),
        }
    }

    fn unwrap_qbit(config: &DownloaderConfig) -> &DownloadConfig<QbitConfig> {
        match config {
            DownloaderConfig::Qbit(c) => c,
            DownloaderConfig::Default(_) => panic!("test expects qbit config"),
        }
    }

    #[test]
    fn qbit_config_exposes_name_base_path_and_active() {
        let config = qbit_config("qb-main", true, "/data/anime", "s3cret");
        assert!(config.is_active());
        assert_eq!(config.name(), "qb-main");
        assert_eq!(config.base_path(), "/data/anime");
    }

    #[test]
    fn default_config_exposes_name_base_path_and_active() {
        let config = default_config("default-main", false, "downloads");
        assert!(!config.is_active());
        assert_eq!(config.name(), "default-main");
        assert_eq!(config.base_path(), "downloads");
    }

    #[test]
    fn set_active_toggles_both_variants() {
        let mut qbit = qbit_config("qb", false, "/data", "s3cret");
        qbit.set_active(true);
        assert!(qbit.is_active());

        let mut default = default_config("default", true, "/data");
        default.set_active(false);
        assert!(!default.is_active());
    }

    #[test]
    fn sanitized_qbit_masks_password_and_keeps_other_fields() {
        let original = qbit_config("qb-main", true, "/data/anime", "s3cret-password");
        let sanitized = original.clone().sanitized();

        assert_ne!(password_of(&sanitized), "s3cret-password");
        let inner = unwrap_qbit(&sanitized);
        assert_eq!(inner.config.password, "************");
        assert_eq!(inner.name, "qb-main");
        assert_eq!(inner.base_path, "/data/anime");
        assert!(inner.active);
        assert_eq!(inner.config.username, "admin");
        assert_eq!(inner.config.url, "http://127.0.0.1:8080/");
        // sanitized 是拷贝语义，原配置不受影响
        assert_eq!(password_of(&original), "s3cret-password");
    }

    #[test]
    fn sanitized_default_config_is_unchanged() {
        let original = default_config("default-main", true, "/data");
        let sanitized = original.clone().sanitized();
        assert_eq!(sanitized, original);
    }

    #[test]
    fn encrypt_then_decrypt_secrets_round_trips_with_real_crypto() {
        let provider = AesCryptoProvider::new(TEST_KEY);
        let plain_password = "qbit-p@ssw0rd-密码";
        let mut config = qbit_config("qb-main", true, "/data", plain_password);

        config
            .encrypt_secrets(&provider)
            .expect("encrypting password should not fail");
        let cipher = password_of(&config);
        assert_ne!(cipher, plain_password);
        assert!(
            !cipher.contains(plain_password),
            "ciphertext should not contain plaintext password, actual: {cipher}"
        );
        // 其余项不参与加密
        assert_eq!(unwrap_qbit(&config).config.username, "admin");

        config
            .decrypt_secrets(&provider)
            .expect("decrypting password should not fail");
        assert_eq!(password_of(&config), plain_password);
    }

    #[test]
    fn encrypt_secrets_uses_random_nonce_so_ciphertexts_differ() {
        let provider = AesCryptoProvider::new(TEST_KEY);
        let mut first = qbit_config("qb", true, "/data", "same-password");
        let mut second = qbit_config("qb", true, "/data", "same-password");

        first
            .encrypt_secrets(&provider)
            .expect("encrypting password should not fail");
        second
            .encrypt_secrets(&provider)
            .expect("encrypting password should not fail");

        assert_ne!(password_of(&first), password_of(&second));
    }

    #[test]
    fn default_config_secret_operations_are_no_ops() {
        let provider = AesCryptoProvider::new(TEST_KEY);
        let mut config = default_config("default", true, "/data");
        let before = config.clone();

        config
            .encrypt_secrets(&provider)
            .expect("encrypting default config should succeed directly");
        config
            .decrypt_secrets(&provider)
            .expect("decrypting default config should succeed directly");

        assert_eq!(config, before);
    }

    #[test]
    fn user_role_converts_to_and_from_u8() {
        assert_eq!(u8::from(UserRole::Admin), 1);
        assert_eq!(u8::from(UserRole::User), 2);
        assert_eq!(
            UserRole::try_from(1).expect("1 should parse as Admin"),
            UserRole::Admin
        );
        assert_eq!(
            UserRole::try_from(2).expect("2 should parse as User"),
            UserRole::User
        );

        let err = UserRole::try_from(7).expect_err("unknown role should fail");
        assert!(
            err.to_string().contains("unknown user role 7"),
            "actual error: {err}"
        );
    }
}
