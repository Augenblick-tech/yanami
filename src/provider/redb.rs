use anyhow::{Error, Ok};
use redb::{Database, TableDefinition};

use crate::models::user::UserEntity;

use super::provider::Provider;

struct UserTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> UserTable<'a> {
    pub fn to_id_key(&self, id: i64) -> String {
        format!("/user/{}", id)
    }
    pub fn to_username_key(&self, username: &str) -> String {
        format!("/user/{}", username)
    }
}

pub struct ReDB<'a> {
    client: redb::Database,
    user: UserTable<'a>,
}

impl<'a> ReDB<'a> {
    pub fn new(path: String) -> Result<Self, anyhow::Error> {
        let user = UserTable {
            table: TableDefinition::new("user_table"),
        };
        let client = Database::create(path)?;
        Ok(ReDB { user, client })
    }
}

impl<'a> Provider for ReDB<'a> {
    fn set_user(&self, user: UserEntity) -> Result<(), anyhow::Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.user.table)?;
            table.insert(self.user.to_id_key(user.id), user.to_vec()?)?;
            table.insert(
                self.user.to_username_key(user.username.as_str()),
                self.user.to_id_key(user.id).into_bytes(),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_user(&self, id: i64) -> Result<UserEntity, anyhow::Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.user.table)?;
        let r = table.get(self.user.to_id_key(id))?;
        if let Some(r) = r {
            Ok(UserEntity::from_slice(&r.value())?)
        } else {
            Err(Error::msg("not found user from id"))
        }
    }

    fn get_user_from_username(&self, username: &str) -> Result<UserEntity, Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.user.table)?;
        let r = table.get(self.user.to_username_key(username))?;
        if r.is_none() {
            return Err(Error::msg("not found user from username"));
        }
        let key = String::from_utf8(r.unwrap().value())?;
        let r = table.get(key)?;
        if r.is_none() {
            return Err(Error::msg("not found user from id"));
        }
        Ok(UserEntity::from_slice(&r.unwrap().value())?)
    }
}
