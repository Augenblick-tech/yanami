use anyhow::Error;

use crate::models::{
    rss::{RSSReq, RSS},
    user::{RegisterCode, UserEntity},
};

pub trait Provider {
    fn is_empty(&self) -> Result<bool, Error>;
    fn update_user(&self, user: UserEntity) -> Result<(), Error>;
    fn create_user(&self, user: UserEntity) -> Result<UserEntity, Error>;
    fn get_user(&self, id: i64) -> Result<Option<UserEntity>, Error>;
    fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error>;
    fn get_users(&self) -> Result<Option<Vec<UserEntity>>, Error>;

    fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error>;
    fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error>;

    fn set_rss(&self, rss: RSSReq) -> Result<RSS, Error>;
    fn del_rss(&self, id: String) -> Result<(), Error>;
    fn get_rss(&self, id: String) -> Result<Option<RSS>, Error>;
    fn get_all_rss(&self) -> Result<Option<Vec<RSS>>, Error>;
}
