use std::sync::Arc;

use anna::{anime::tracker::AnimeInfo, qbit::qbitorrent::QbitConfig};
use anyhow::Error;

use async_trait::async_trait;
use model::{
    anime::{AnimeStatus, AnimesQuertOption},
    rss::{AnimeRssRecord, RSSReq, RssRecord, RSS},
    rule::Rule,
    user::{RegisterCode, UserEntity},
};

pub type UserProvider = Arc<dyn User + Send + Sync>;
pub type DbProvider = Arc<dyn Db + Send + Sync>;
pub type RssProvider = Arc<dyn Rss + Send + Sync>;
pub type RuleProvider = Arc<dyn Rules + Send + Sync>;
pub type AnimeProvider = Arc<dyn Anime + Send + Sync>;
pub type ServiceConfigProvider = Arc<dyn ServiceConfig + Send + Sync>;

#[async_trait]
pub trait Db {
    async fn is_empty(&self) -> Result<bool, Error>;
}

#[async_trait]
pub trait User {
    async fn update_user(&self, user: UserEntity) -> Result<(), Error>;
    async fn create_user(&self, user: UserEntity) -> Result<UserEntity, Error>;
    async fn get_user(&self, id: i64) -> Result<Option<UserEntity>, Error>;
    async fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error>;
    async fn get_users(&self) -> Result<Option<Vec<UserEntity>>, Error>;
    async fn edit_password(&self, id: i64, password: &str) -> anyhow::Result<()>;

    async fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error>;
    async fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error>;
}

#[async_trait]
pub trait Rss {
    async fn set_rss(&self, rss: RSSReq) -> Result<RSS, Error>;
    async fn del_rss(&self, id: String) -> Result<(), Error>;
    async fn get_rss(&self, id: String) -> Result<Option<RSS>, Error>;
    async fn get_all_rss(&self) -> Result<Option<Vec<RSS>>, Error>;
    // insert_or_update_rss_record 插入并更新RSS记录
    async fn insert_or_update_rss_record(&self, i: &RssRecord) -> Result<(), Error>;
    // select_latest_rss_records 获取最近三个小时的记录
    async fn select_latest_rss_records(&self) -> Result<Vec<RssRecord>, Error>;
    // get_rss_record_by_url 根据URL获取RSS记录
    async fn get_rss_record_by_url(&self, url: &str) -> Result<Option<RssRecord>, Error>;
    // search_rss_records_by_keywords 根据关键词搜索RSS记录
    async fn search_rss_records_by_keywords(
        &self,
        keywords: &[String],
    ) -> Result<Vec<RssRecord>, Error>;
}

#[async_trait]
pub trait Anime {
    // 覆盖所有存在id的信息，不存在id的则创建，如果is_lock为true这跳过覆盖
    async fn set_calenders(&self, calender: Vec<AnimeInfo>) -> Result<(), Error>;

    // 忽略is_lock
    async fn set_calender(&self, anime_status: AnimeStatus) -> Result<(), Error>;
    async fn get_calenders(&self) -> Result<Option<Vec<AnimeStatus>>, Error>;
    async fn get_calender(&self, id: i64) -> Result<Option<AnimeStatus>, Error>;
    async fn search_calender(
        &self,
        name: String,
        option: Option<AnimesQuertOption>,
    ) -> Result<Option<Vec<AnimeStatus>>, Error>;
    async fn get_calenders_with_query(
        &self,
        option: Option<AnimesQuertOption>,
    ) -> Result<Vec<AnimeStatus>, Error>;

    async fn set_anime_recode(
        &self,
        anime_id: i64,
        anime_rss_record: AnimeRssRecord,
    ) -> Result<(), Error>;
    async fn get_anime_record(
        &self,
        anime_id: i64,
        info_hash: &str,
    ) -> Result<Option<AnimeRssRecord>, Error>;
    async fn get_anime_rss_recodes(
        &self,
        anime_id: i64,
    ) -> Result<Option<Vec<AnimeRssRecord>>, Error>;
    async fn latest_anime_records(&self, n: i64) -> Result<Vec<AnimeRssRecord>, Error>;
}

#[async_trait]
pub trait Rules {
    async fn set_rule(&self, rule: Rule) -> Result<(), Error>;
    async fn del_rule(&self, name: String) -> Result<(), Error>;
    async fn get_rule(&self, name: String) -> Result<Option<Rule>, Error>;
    async fn get_all_rules(&self) -> Result<Option<Vec<Rule>>, Error>;
}

#[async_trait]
pub trait ServiceConfig {
    async fn set_path(&self, path: &str) -> Result<(), Error>;
    async fn get_path(&self) -> Result<Option<String>, Error>;

    async fn set_qbit(&self, url: &str, username: &str, password: &str) -> Result<(), Error>;
    async fn get_qbit(&self) -> Result<Option<QbitConfig>, Error>;
}
