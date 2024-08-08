use anyhow::Error;

use crate::models::user::{RegisterCode, UserEntity};

pub trait Provider {
    fn update_user(&self, user: UserEntity) -> Result<(), Error>;
    fn create_user(&self, user: UserEntity) -> Result<UserEntity, Error>;
    fn get_user(&self, id: i64) -> Result<Option<UserEntity>, Error>;
    fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error>;

    fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error>;
    fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error>;
}
