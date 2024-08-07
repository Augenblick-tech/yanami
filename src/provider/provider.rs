use anyhow::Error;

use crate::models::user::UserEntity;

pub trait Provider {
    fn set_user(&self, user: UserEntity) -> Result<(), Error>;
    fn get_user(&self, id: i64) -> Result<UserEntity, Error>;
    fn get_user_from_username(&self, username: &str) -> Result<UserEntity, Error>;
}
