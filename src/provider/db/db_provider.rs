use std::sync::Arc;

use anna::{anime::tracker::AnimeInfo, qbit::qbitorrent::QbitConfig};
use anyhow::Error;

use crate::models::{
    anime::AnimeStatus,
    rss::{AnimeRssRecord, RSSReq, RSS},
    rule::Rule,
    user::{RegisterCode, UserEntity},
};

pub type UserProvider = Arc<dyn User + Send + Sync>;
pub type DbProvider = Arc<dyn Db + Send + Sync>;
pub type RssProvider = Arc<dyn Rss + Send + Sync>;
pub type RuleProvider = Arc<dyn Rules + Send + Sync>;
pub type AnimeProvider = Arc<dyn Anime + Send + Sync>;
pub type ServiceConfigProvider = Arc<dyn ServiceConfig + Send + Sync>;

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
    fn set_calenders(&self, calender: Vec<AnimeInfo>) -> Result<(), Error>;
    fn set_calender(&self, anime_status: AnimeStatus) -> Result<(), Error>;
    fn get_calenders(&self) -> Result<Option<Vec<AnimeStatus>>, Error>;
    fn get_calender(&self, id: i64) -> Result<Option<AnimeStatus>, Error>;
    fn search_calender(&self, name: String) -> Result<Option<Vec<AnimeStatus>>, Error>;

    fn set_anime_recode(
        &self,
        anime_id: i64,
        anime_rss_record: AnimeRssRecord,
    ) -> Result<(), Error>;
    fn get_anime_record(
        &self,
        anime_id: i64,
        info_hash: &str,
    ) -> Result<Option<AnimeRssRecord>, Error>;
    fn get_anime_rss_recodes(&self, anime_id: i64) -> Result<Option<Vec<AnimeRssRecord>>, Error>;
}

pub trait Rules {
    fn set_rule(&self, rule: Rule) -> Result<(), Error>;
    fn del_rule(&self, name: String) -> Result<(), Error>;
    fn get_rule(&self, name: String) -> Result<Option<Rule>, Error>;
    fn get_all_rules(&self) -> Result<Option<Vec<Rule>>, Error>;
}

pub trait ServiceConfig {
    fn set_path(&self, path: &str) -> Result<(), Error>;
    fn get_path(&self) -> Result<Option<String>, Error>;

    fn set_qbit(&self, url: &str, username: &str, password: &str) -> Result<(), Error>;
    fn get_qbit(&self) -> Result<Option<QbitConfig>, Error>;
}
