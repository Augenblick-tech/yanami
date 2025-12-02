use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct Model {
    pub key: ConfigKey,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigKey {
    DownloadPath,
    QbitConfig,
    None,
}

impl From<&str> for ConfigKey {
    fn from(value: &str) -> Self {
        match value {
            "download_path" => ConfigKey::DownloadPath,
            "qbit_config" => ConfigKey::QbitConfig,
            _ => ConfigKey::None,
        }
    }
}

impl From<ConfigKey> for String {
    fn from(val: ConfigKey) -> Self {
        match val {
            ConfigKey::QbitConfig => String::from("qbit_config"),
            ConfigKey::DownloadPath => String::from("download_path"),
            ConfigKey::None => String::from(""),
        }
    }
}
