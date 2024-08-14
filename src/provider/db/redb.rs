use anna::anime::anime::AnimeInfo;
use anyhow::Error;
use chrono::Local;
use redb::{Database, ReadableTable, TableDefinition, TableError};

use crate::models::{
    rss::{AnimeRssRecord, RSSReq, RSS},
    rule::GroupRule,
    user::{RegisterCode, UserEntity},
};

use super::db_provider::{Anime, Db, Rss, Rules, User};

struct UserTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
    pwd: TableDefinition<'a, String, String>,
    id: TableDefinition<'a, String, i64>,
}

impl<'a> UserTable<'a> {
    fn to_id_key(&self, id: i64) -> String {
        format!("/user/id/{}", id)
    }
    fn to_username_key(&self, username: &str) -> String {
        format!("/user/username/{}", username)
    }

    fn to_uid_key(&self) -> String {
        "/user_id".to_string()
    }
}

struct UserRegisterTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> UserRegisterTable<'a> {
    pub fn to_key(&self, code: &str) -> String {
        format!("/user/register/{}", code)
    }
}

struct RSSTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> RSSTable<'a> {
    pub fn to_key(&self, id: &str) -> String {
        format!("/rss/id/{}", id)
    }
}

struct CalenderTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> CalenderTable<'a> {
    pub fn to_key(&self, id: i64) -> String {
        format!("{}", id)
    }
}

struct RuleTable<'a> {
    table: TableDefinition<'a, String, Vec<u8>>,
}

impl<'a> RuleTable<'a> {
    pub fn to_key(&self, id: &str) -> String {
        format!("{}", id)
    }
}

pub struct ReDB<'a> {
    client: redb::Database,
    user: UserTable<'a>,
    register: UserRegisterTable<'a>,
    rss: RSSTable<'a>,
    anime_calender: CalenderTable<'a>,
    rule: RuleTable<'a>,
}

impl<'a> ReDB<'a> {
    pub fn new(path: String) -> Result<Self, anyhow::Error> {
        let user = UserTable {
            table: TableDefinition::new("user_table"),
            pwd: TableDefinition::new("user_pwd_table"),
            id: TableDefinition::new("user_id_table"),
        };
        let client = Database::create(path)?;
        Ok(ReDB {
            user,
            client,
            register: UserRegisterTable {
                table: TableDefinition::new("user_register_table"),
            },
            rss: RSSTable {
                table: TableDefinition::new("rss_table"),
            },
            anime_calender: CalenderTable {
                table: TableDefinition::new("anime_calender"),
            },
            rule: RuleTable {
                table: TableDefinition::new("rule_table"),
            },
        })
    }
}

impl<'a> Db for ReDB<'a> {
    fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.client.begin_read()?.list_tables()?.count() <= 0)
    }
}
impl<'a> User for ReDB<'a> {
    fn update_user(&self, user: UserEntity) -> Result<(), anyhow::Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.user.table)?;
            table.insert(self.user.to_id_key(user.id), user.to_vec()?)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn create_user(&self, mut user: UserEntity) -> Result<UserEntity, Error> {
        if user.id <= 0 {
            let tx = self.client.begin_read()?;
            let table = tx.open_table(self.user.id);
            user.id = match table {
                Ok(table) => {
                    let r = table.get(self.user.to_uid_key())?;
                    if let Some(r) = r {
                        r.value()
                    } else {
                        10000
                    }
                }
                Err(TableError::TableDoesNotExist(_)) => 10000,
                Err(e) => return Err(Error::msg(e.to_string())),
            }
        }
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.user.table)?;
            let mut pwd = tx.open_table(self.user.pwd)?;
            let mut id = tx.open_table(self.user.id)?;
            table.insert(self.user.to_id_key(user.id), user.to_vec()?)?;
            pwd.insert(
                self.user.to_username_key(user.username.as_str()),
                self.user.to_id_key(user.id),
            )?;
            id.insert(self.user.to_uid_key(), user.id + 1)?;
        }
        tx.commit()?;
        Ok(user)
    }

    fn get_user(&self, id: i64) -> Result<Option<UserEntity>, anyhow::Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.user.table);
        match table {
            Ok(table) => {
                let r = table.get(self.user.to_id_key(id))?;
                if let Some(r) = r {
                    Ok(Some(UserEntity::from_slice(&r.value())?))
                } else {
                    Ok(None)
                }
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error> {
        let tx = self.client.begin_read()?;
        let pwd = tx.open_table(self.user.pwd);
        match pwd {
            Ok(pwd) => {
                let r = pwd.get(self.user.to_username_key(username))?;
                if r.is_none() {
                    return Ok(None);
                }
                let table = tx.open_table(self.user.table);
                match table {
                    Ok(table) => {
                        let key = r.unwrap().value();
                        let r = table.get(key)?;
                        if r.is_none() {
                            Ok(None)
                        } else {
                            Ok(Some(UserEntity::from_slice(&r.unwrap().value())?))
                        }
                    }
                    Err(TableError::TableDoesNotExist(_)) => Ok(None),
                    Err(e) => Err(Error::msg(e.to_string())),
                }
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn get_users(&self) -> Result<Option<Vec<UserEntity>>, Error> {
        if self.is_empty()? {
            return Ok(None);
        }
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.user.table);
        match table {
            Ok(table) => {
                let mut users = Vec::<UserEntity>::new();
                for data in table.iter()? {
                    let user = UserEntity::from_slice(&data?.1.value());
                    if user.is_ok() {
                        let mut user = user.unwrap();
                        user.password.clear();
                        users.push(user);
                    } else {
                        tracing::debug!("get users failed, err: {:?}", user.unwrap_err())
                    }
                }

                Ok(Some(users))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.register.table)?;
            let data = serde_json::to_vec(&registry)?;
            table.insert(self.register.to_key(&registry.code), data)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.register.table);
        match table {
            Ok(table) => {
                let r = table.get(self.register.to_key(&code))?;
                if r.is_none() {
                    return Ok(None);
                }
                let a: RegisterCode = serde_json::from_slice(&r.unwrap().value().to_vec())?;
                let now = Local::now().to_utc().timestamp();
                if a.now + a.expire <= now || a.timers <= 0 {
                    let tx = self.client.begin_write()?;
                    {
                        let mut table = tx.open_table(self.register.table)?;
                        table.remove(self.register.to_key(&a.code))?;
                    }
                    tx.commit()?;
                    return Ok(None);
                }

                Ok(Some(a))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }
}

impl<'a> Rss for ReDB<'a> {
    fn set_rss(&self, req: RSSReq) -> Result<RSS, Error> {
        let tx = self.client.begin_write()?;
        let key = format!("{:x}", md5::compute(req.url.to_string()));
        let rss = RSS {
            id: key.to_string(),
            url: req.url,
            title: req.title,
            search_url: req.search_url,
        };
        {
            let mut table = tx.open_table(self.rss.table)?;
            tracing::debug!("set rss id:{}, url:{}", &key, &rss.url);
            table.insert(self.rss.to_key(&key), serde_json::to_vec(&rss)?)?;
        }
        tx.commit()?;
        Ok(rss)
    }

    fn del_rss(&self, id: String) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            tracing::debug!("delete rss {}", id);
            let mut table = tx.open_table(self.rss.table)?;
            table.remove(self.rss.to_key(&id))?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_rss(&self, id: String) -> Result<Option<RSS>, Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.rss.table);
        match table {
            Ok(table) => {
                let r = table.get(self.rss.to_key(&id))?;
                if let Some(r) = r {
                    Ok(Some(serde_json::from_slice(&r.value())?))
                } else {
                    Ok(None)
                }
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn get_all_rss(&self) -> Result<Option<Vec<RSS>>, Error> {
        if self.is_empty()? {
            return Ok(None);
        }
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.rss.table);
        let mut rss_list = Vec::<RSS>::new();
        match table {
            Ok(table) => {
                for data in table.iter()? {
                    let data = data?;
                    let rss = serde_json::from_slice(&data.1.value());
                    if rss.is_ok() {
                        tracing::debug!("get all rss {}", &data.0.value());
                        rss_list.push(rss.unwrap());
                    }
                }
                Ok(Some(rss_list))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }
}

impl<'a> Anime for ReDB<'a> {
    fn set_calender(&self, calender: Vec<anna::anime::anime::AnimeInfo>) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.anime_calender.table)?;
            for i in calender {
                table.insert(self.anime_calender.to_key(i.id), serde_json::to_vec(&i)?)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn get_calender(&self) -> Result<Option<Vec<anna::anime::anime::AnimeInfo>>, Error> {
        if self.is_empty()? {
            return Ok(None);
        }
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.anime_calender.table);
        let mut calender = Vec::<AnimeInfo>::new();
        match table {
            Ok(table) => {
                for data in table.iter()? {
                    let data = data?;
                    let rss = serde_json::from_slice(&data.1.value());
                    if rss.is_ok() {
                        calender.push(rss.unwrap());
                    }
                }
                Ok(Some(calender))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn set_anime_rss(
        &self,
        anime_name: String,
        anime_rss_record: crate::models::rss::AnimeRssRecord,
    ) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table =
                tx.open_table(TableDefinition::<String, Vec<u8>>::new(anime_name.as_str()))?;
            table.insert(
                anime_rss_record.title.clone(),
                serde_json::to_vec(&anime_rss_record)?,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_anime_rss(
        &self,
        anime_name: String,
    ) -> Result<Option<Vec<crate::models::rss::AnimeRssRecord>>, Error> {
        if self.is_empty()? {
            return Ok(None);
        }
        let tx = self.client.begin_read()?;
        let table = tx.open_table(TableDefinition::<String, Vec<u8>>::new(anime_name.as_str()));
        let mut list = Vec::<AnimeRssRecord>::new();
        match table {
            Ok(table) => {
                for data in table.iter()? {
                    let data = data?;
                    let rss = serde_json::from_slice(&data.1.value());
                    if rss.is_ok() {
                        list.push(rss.unwrap());
                    }
                }
                Ok(Some(list))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }
}

impl<'a> Rules for ReDB<'a> {
    fn set_rule(&self, rule: crate::models::rule::GroupRule) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.rule.table)?;
            table.insert(
                self.rule.to_key(rule.name.as_str()),
                serde_json::to_vec(&rule)?,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn del_rule(&self, name: String) -> Result<(), Error> {
        let tx = self.client.begin_write()?;
        {
            let mut table = tx.open_table(self.rule.table)?;
            table.remove(self.rule.to_key(name.as_str()))?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_rule(&self, name: String) -> Result<Option<GroupRule>, Error> {
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.rule.table);
        match table {
            Ok(table) => {
                let r = table.get(self.rule.to_key(&name))?;
                if let Some(r) = r {
                    Ok(Some(serde_json::from_slice(&r.value())?))
                } else {
                    Ok(None)
                }
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }

    fn get_all_rules(&self) -> Result<Option<Vec<GroupRule>>, Error> {
        if self.is_empty()? {
            return Ok(None);
        }
        let tx = self.client.begin_read()?;
        let table = tx.open_table(self.rule.table);
        let mut group_rules = Vec::<GroupRule>::new();
        match table {
            Ok(table) => {
                for data in table.iter()? {
                    let data = data?;
                    let rss = serde_json::from_slice(&data.1.value());
                    if rss.is_ok() {
                        group_rules.push(rss.unwrap());
                    }
                }
                Ok(Some(group_rules))
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(Error::msg(e.to_string())),
        }
    }
}
