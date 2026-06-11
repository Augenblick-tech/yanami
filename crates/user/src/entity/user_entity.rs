use anyhow::anyhow;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use common::shared::error::Error;

use crate::entity::model::{DownloaderConfig, UserBaseData, UserRole};

#[derive(Clone)]
pub struct UserEntity {
    data: UserBaseData,
}

impl UserEntity {
    pub(super) fn new(data: UserBaseData) -> Self {
        Self { data }
    }

    pub(super) fn get_base_data(&self) -> &UserBaseData {
        &self.data
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn username(&self) -> &str {
        &self.data.username
    }

    pub fn role(&self) -> UserRole {
        self.data.role
    }

    pub fn space_id(&self) -> i64 {
        self.data.space_id
    }

    pub fn auto_sub(&self) -> bool {
        self.data.auto_sub
    }

    // TODO: 返回加密密码
    pub fn get_download_config(&self) -> &Vec<DownloaderConfig> {
        &self.data.download_config
    }

    // TODO: 返回明文密码
    pub fn download_config(&self) -> Option<&DownloaderConfig> {
        self.data.download_config.iter().find(|i| i.is_active())
    }

    pub fn delete_download_config(&mut self, config_name: &str) {
        self.data
            .download_config
            .retain(|c| c.name() != config_name);
    }

    pub fn enable_download_config(&mut self, config_name: &str) -> Result<(), Error> {
        let mut found = false;

        for config in &mut self.data.download_config {
            if config.name() == config_name {
                config.set_active(true);
                found = true;
            } else if config.is_active() {
                config.set_active(false);
            }
        }

        if !found {
            return Err(Error::conflict(format!(
                "not found download config {}",
                config_name
            )));
        }

        Ok(())
    }

    // TODO: 接收明文密码，加密后存储
    pub fn save_download_config(&mut self, config: DownloaderConfig) {
        let new_is_active = config.is_active();
        let target_name = config.name().to_string();
        let mut target_index = None;

        for (i, existing) in self.data.download_config.iter_mut().enumerate() {
            if existing.name() == target_name {
                target_index = Some(i);
            } else if new_is_active && existing.is_active() {
                existing.set_active(false);
            }
        }

        if let Some(index) = target_index {
            self.data.download_config[index] = config;
        } else {
            self.data.download_config.push(config);
        }
    }

    pub fn enable_auto_sub_anime(&mut self) {
        self.data.auto_sub = true;
    }

    pub fn disable_auto_sub_anime(&mut self) {
        self.data.auto_sub = false;
    }

    pub fn verify_password(&self, plain_password: &str) -> Result<bool, Error> {
        let hash = PasswordHash::new(&self.data.password)
            .map_err(|_| Error::invariant("user verify password failed, unknown password"))?;
        if let Ok(()) = Argon2::default().verify_password(plain_password.as_bytes(), &hash) {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn set_password(&mut self, pwd: &str) -> Result<(), Error> {
        let pwd_hash = Self::hash_password(pwd)?;
        self.data.password = pwd_hash;
        Ok(())
    }

    pub(super) fn hash_password(password: &str) -> Result<String, Error> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| Error::external("hash password failed", anyhow!(e)))?
            .to_string())
    }
}
