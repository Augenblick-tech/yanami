use std::sync::Arc;

use anna::anime::anime::AnimeInfo;
use anyhow::Error;

use crate::models::{
    rss::{AnimeRssRecord, RSSReq, RSS},
    rule::GroupRule,
    user::{RegisterCode, UserEntity},
};

pub type UserProvider = Arc<dyn User + Send + Sync>;
pub type DbProvider = Arc<dyn Db + Send + Sync>;
pub type RssProvider = Arc<dyn Rss + Send + Sync>;
pub type RuleProvider = Arc<dyn Rules + Send + Sync>;
pub type AnimeProvider = Arc<dyn Anime + Send + Sync>;
pub type DownloadPathProvider = Arc<dyn DownloadPath + Send + Sync>;

pub trait Db {
    fn is_empty(&self) -> Result<bool, Error>;
}

pub trait User {
    fn update_user(&self, user: UserEntity) -> Result<(), Error>;
    fn create_user(&self, user: UserEntity) -> Result<UserEntity, Error>;
    fn get_user(&self, id: i64) -> Result<Option<UserEntity>, Error>;
    fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error>;
    fn get_users(&self) -> Result<Option<Vec<UserEntity>>, Error>;

    fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error>;
    fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error>;
}

pub trait Rss {
    fn set_rss(&self, rss: RSSReq) -> Result<RSS, Error>;
    fn del_rss(&self, id: String) -> Result<(), Error>;
    fn get_rss(&self, id: String) -> Result<Option<RSS>, Error>;
    fn get_all_rss(&self) -> Result<Option<Vec<RSS>>, Error>;
}

pub trait Anime {
    fn set_calender(&self, calender: Vec<AnimeInfo>) -> Result<(), Error>;
    fn get_calender(&self) -> Result<Option<Vec<AnimeInfo>>, Error>;

    fn set_anime_rss(
        &self,
        anime_name: String,
        anime_rss_record: AnimeRssRecord,
    ) -> Result<(), Error>;
    fn get_anime_rss(&self, anime_name: String) -> Result<Option<Vec<AnimeRssRecord>>, Error>;
}

pub trait Rules {
    fn set_rule(&self, rule: GroupRule) -> Result<(), Error>;
    fn del_rule(&self, name: String) -> Result<(), Error>;
    fn get_rule(&self, name: String) -> Result<Option<GroupRule>, Error>;
    fn get_all_rules(&self) -> Result<Option<Vec<GroupRule>>, Error>;
}

pub trait DownloadPath {
    fn set_path(&self, path: &str) -> Result<(), Error>;
    fn get_path(&self) -> Result<Option<String>, Error>;
}
