use anyhow::Error;

use crate::models::user::{Register, UserEntity};

pub trait Provider {
    fn set_user(&self, user: UserEntity) -> Result<(), Error>;
    fn get_user(&self, id: i64) -> Result<UserEntity, Error>;
    fn get_user_from_username(&self, username: &str) -> Result<UserEntity, Error>;

    fn set_register_code(&self, registry: Register) -> Result<(), Error>;
    fn get_register_code(&self, code: String) -> Result<Option<Register>, Error>;
}
