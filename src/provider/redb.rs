use anyhow::Ok;
use redb::{Database, TableDefinition};

use crate::models::user::UserEntity;

use super::provider::Provider;

struct UserTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> UserTable<'a> {
    pub fn to_key(&self, id: i64) -> String {
        format!("/user/{}", id)
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
            table.insert(self.user.to_key(user.id), &serde_json::to_vec(&user)?)?;
        }
        tx.commit()?;
        Ok(())
    }
}
