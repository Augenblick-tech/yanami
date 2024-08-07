use anyhow::{Error, Ok};
use chrono::Local;
use redb::{Database, TableDefinition};

use crate::models::user::{Register, UserEntity};

use super::db_provider::Provider;

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

struct UserRegisterTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> UserRegisterTable<'a> {
    pub fn to_key(&self, code: String) -> String {
        format!("/user/register/{}", code)
    }
}
pub struct ReDB<'a> {
    client: redb::Database,
    user: UserTable<'a>,
    register: UserRegisterTable<'a>,
}

impl<'a> ReDB<'a> {
    pub fn new(path: String) -> Result<Self, anyhow::Error> {
        let user = UserTable {
            table: TableDefinition::new("user_table"),
        };
        let client = Database::create(path)?;
        Ok(ReDB {
            user,
            client,
            register: UserRegisterTable {
                table: TableDefinition::new("user_register_table"),
            },
        })
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

    fn set_register_code(&self, registry: Register) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.register.table)?;
            let data = serde_json::to_vec(&registry)?;
            table.insert(self.register.to_key(registry.code), data)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_register_code(&self, code: String) -> Result<Option<Register>, Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.register.table)?;
        let r = table.get(self.register.to_key(code))?;
        if r.is_none() {
            return Ok(None);
        }
        let a: Register = serde_json::from_slice(&r.unwrap().value().to_vec())?;
        let now = Local::now().to_utc().timestamp();
        if a.now + a.expire <= now {
            let tx = self.client.begin_write()?;
            {
                let mut table = tx.open_table(self.register.table)?;
                table.remove(self.register.to_key(a.code))?;
            }
            tx.commit()?;
            return Ok(None);
        }

        return Ok(Some(a));
    }
}
