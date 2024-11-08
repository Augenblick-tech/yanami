use sea_orm::entity::prelude::*;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, FromRow)]
#[sea_orm(table_name = "config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub key: ConfigKey,
    pub value: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "key")]
pub enum ConfigKey {
    #[sea_orm(string_value = "download_path")]
    DownloadPath,
    #[sea_orm(string_value = "qbit_config")]
    QbitConfig,
    #[sea_orm(string_value = "")]
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
