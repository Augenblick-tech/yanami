use anyhow::Error;

use crate::models::user::UserEntity;

pub trait Provider {
    fn set_user(&self, user: UserEntity) -> Result<(), Error>;
}
