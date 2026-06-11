use common::shared::error::Error;
use std::sync::Arc;

use crate::entity::{
    cap::{DownloaderManager, UserRepository},
    downloader::Downloader,
    model::UserRole,
    user_entity::UserEntity,
};

#[derive(Clone)]
pub struct Users {
    repo: Arc<dyn UserRepository>,
    downloader_manager: Arc<dyn DownloaderManager>,
}

impl Users {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        downloader_manager: Arc<dyn DownloaderManager>,
    ) -> Self {
        Self {
            repo,
            downloader_manager,
        }
    }
}

impl Users {
    pub async fn get(&self, id: i64) -> Result<Option<UserEntity>, Error> {
        let props = self
            .repo
            .find(id)
            .await
            .map_err(|e| Error::external("users get user failed", e))?;
        if let Some(props) = props {
            Ok(Some(UserEntity::new(props.data)))
        } else {
            Ok(None)
        }
    }

    pub async fn get_by_username(&self, username: &str) -> Result<Option<UserEntity>, Error> {
        let user = self
            .repo
            .find_by_username(username)
            .await
            .map_err(|e| Error::external("users get user by username failed", e))?;
        if let Some(user) = user {
            Ok(Some(UserEntity::new(user.data)))
        } else {
            Ok(None)
        }
    }

    pub async fn save(&self, entity: &UserEntity) -> Result<(), Error> {
        self.repo
            .update(entity.get_base_data())
            .await
            .map_err(|e| Error::external("save user failed", e))?;
        Ok(())
    }

    pub async fn create(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
        auto_sub: bool,
    ) -> Result<UserEntity, Error> {
        let pwd = UserEntity::hash_password(password)?;
        let props = self
            .repo
            .insert(username, &pwd, role, auto_sub)
            .await
            .map_err(|e| Error::external("users create user failed", e))?;
        Ok(UserEntity::new(props.data))
    }

    pub async fn list_auto_sub(&self) -> Result<Vec<UserEntity>, Error> {
        Ok(self
            .repo
            .list_auto_sub()
            .await
            .map_err(|e| Error::external("users list auto sub failed", e))?
            .into_iter()
            .map(|i| UserEntity::new(i.data))
            .collect())
    }

    pub async fn get_by_space_id(&self, space_id: i64) -> Result<Option<UserEntity>, Error> {
        Ok(self
            .repo
            .find_by_space_id(space_id)
            .await
            .map_err(|e| Error::external("users get user by space_id failed", e))?.map(|i| UserEntity::new(i.data)))
    }

    // 初始化管理员用户，用户名: moexco，密码随机，输出到标准输出
    pub async fn init_admin_user(&self) -> Result<(), Error> {
        let admin_count = self
            .repo
            .count_by_role(UserRole::Admin)
            .await
            .map_err(|e| Error::external("count admin users failed", e))?;

        if admin_count > 0 {
            return Ok(());
        }

        use rand::Rng;
        let password: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

        self.create("moexco", &password, UserRole::Admin, false)
            .await?;

        println!("============================================================");
        println!("system initialized successfully, auto-created initial admin account");
        println!("username: moexco");
        println!("initial password: {}", password);
        println!("please keep it safe or login to change it as soon as possible!");
        println!("============================================================");

        Ok(())
    }
}

impl Users {
    pub async fn as_downloader(&self, entity: &UserEntity) -> Result<Option<Downloader>, Error> {
        let user_id = entity.id();
        let Some(config) = entity.download_config() else {
            return Ok(None);
        };

        let provider = self
            .downloader_manager
            .get(user_id, config)
            .await
            .map_err(|e| Error::external("download manager get provider failed", e))?;
        Ok(Some(Downloader::new(
            user_id,
            config.base_path().to_string(),
            provider,
        )))
    }
}
