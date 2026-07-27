use std::sync::Arc;

use async_trait::async_trait;

use crate::entity::model::{DownloadTask, DownloaderConfig, UserBaseData, UserProps, UserRole};
use anyhow::Result;

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find(&self, id: i64) -> Result<Option<UserProps>>;
    async fn find_by_username(&self, username: &str) -> Result<Option<UserProps>>;
    async fn find_by_space_id(&self, space_id: i64) -> Result<Option<UserProps>>;
    async fn insert(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
        auto_sub: bool,
    ) -> Result<UserProps>;
    async fn update(&self, user: &UserBaseData) -> Result<()>;
    async fn list_auto_sub(&self) -> Result<Vec<UserProps>>;
    async fn count_by_role(&self, role: UserRole) -> Result<i64>;
}

#[async_trait]
pub trait DownloadProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn stop(&self);
    
    async fn download(&self, url: &str, path: &str, hash: [u8; 20]) -> Result<bool>;
    async fn list_task(&self) -> Result<Vec<DownloadTask>>;
    async fn get_task(&self, hash: [u8; 20]) -> Result<Option<DownloadTask>>;
    async fn pause_task(&self, hash: [u8; 20]) -> Result<()>;
    async fn resume_task(&self, hash: [u8; 20]) -> Result<()>;
    async fn delete_task(&self, hash: [u8; 20]) -> Result<()>;
}

#[async_trait]
pub trait DownloaderManager: Send + Sync {
    async fn get(
        &self,
        user_id: i64,
        config: &DownloaderConfig,
    ) -> Result<Arc<dyn DownloadProvider>>;

    async fn validate_config(&self, config: &DownloaderConfig) -> Result<()>;
}

pub trait CryptoProvider: Send + Sync {
    fn encrypt(&self, plain: &str) -> anyhow::Result<String>;
    fn decrypt(&self, cipher: &str) -> anyhow::Result<String>;
}
