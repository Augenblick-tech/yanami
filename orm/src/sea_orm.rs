use ::entity::{anime, anime_record, config, rss, rule};
use anna::qbit::qbitorrent::QbitConfig;
use anyhow::{Error, Result};
use async_trait::async_trait;
use entity::{register_code, user};
use model::user::UserEntity;
use provider::db::{self, Anime, Rss, Rules, ServiceConfig, User};
use sea_orm::{
    prelude::Expr, sea_query::ExprTrait, sqlx::types::chrono::Local, ActiveModelTrait, ColumnTrait,
    Condition, ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set, TransactionTrait, TryIntoModel,
};
use sea_orm_migration::MigratorTrait;
use uuid::Uuid;

use crate::Migrator;

#[derive(Clone)]
pub struct SeaDB {
    conn: DatabaseConnection,
}

impl SeaDB {
    pub async fn new(s: &str) -> Result<Self> {
        let mut opt = ConnectOptions::new(s);
        opt.max_connections(1);
        opt.sqlx_logging(false);
        let conn = Database::connect(opt).await?;
        Ok(SeaDB { conn })
    }

    pub async fn migration(&self) -> Result<()> {
        Ok(Migrator::up(&self.conn, None).await?)
    }
}

#[async_trait]
impl db::Db for SeaDB {
    async fn is_empty(&self) -> std::result::Result<bool, anyhow::Error> {
        self.migration().await?;
        let count = user::Entity::find().count(&self.conn).await?;
        Ok(count == 0)
    }
}

#[async_trait]
impl Anime for SeaDB {
    async fn set_calenders(
        &self,
        calender: Vec<anna::anime::tracker::AnimeInfo>,
    ) -> std::result::Result<(), anyhow::Error> {
        for i in calender {
            let qr = anime::Entity::find_by_id(i.id).one(&self.conn).await;
            if let Ok(status) = qr {
                if let Some(status) = status {
                    if status.is_lock {
                        continue;
                    }
                    if let Ok(value) = serde_json::to_value(&i) {
                        let mut status: anime::ActiveModel = status.into();
                        status.anime_info = Set(value);
                        status.update(&self.conn).await?;
                    }
                } else if let Ok(value) = serde_json::to_value(&i) {
                    anime::ActiveModel {
                        id: Set(i.id),
                        is_lock: Set(false),
                        is_search: Set(false),
                        anime_info: Set(value),
                        status: Set(true),
                        rule_name: Set("".to_string()),
                        progress: Set(0),
                    }
                    .insert(&self.conn)
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn set_calender(
        &self,
        anime_status: model::anime::AnimeStatus,
    ) -> std::result::Result<(), anyhow::Error> {
        if let Some(r) = anime::Entity::find_by_id(anime_status.anime_info.id)
            .one(&self.conn)
            .await?
        {
            let mut ar: anime::ActiveModel = r.into();
            ar.anime_info = Set(serde_json::to_value(&anime_status.anime_info)?);
            ar.is_lock = Set(anime_status.is_lock);
            ar.is_search = Set(anime_status.is_search);
            ar.rule_name = Set(anime_status.rule_name);
            ar.update(&self.conn).await?;
        } else {
            anime::ActiveModel {
                id: Set(anime_status.anime_info.id),
                is_lock: Set(anime_status.is_lock),
                is_search: Set(anime_status.is_search),
                anime_info: Set(serde_json::to_value(&anime_status.anime_info)?),
                status: Set(anime_status.status),
                rule_name: Set(anime_status.rule_name),
                progress: Set((anime_status.progress * 100.0) as u8),
            }
            .insert(&self.conn)
            .await?;
        }

        Ok(())
    }

    async fn get_calenders(
        &self,
    ) -> std::result::Result<Option<Vec<model::anime::AnimeStatus>>, anyhow::Error> {
        Ok(Some(
            anime::Entity::find()
                .all(&self.conn)
                .await?
                .into_iter()
                .map(|r| r.into())
                .collect(),
        ))
    }

    async fn get_calender(
        &self,
        id: i64,
    ) -> std::result::Result<Option<model::anime::AnimeStatus>, anyhow::Error> {
        if let Some(r) = anime::Entity::find_by_id(id).one(&self.conn).await? {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }

    async fn search_calender(
        &self,
        name: String,
    ) -> std::result::Result<Option<Vec<model::anime::AnimeStatus>>, anyhow::Error> {
        let r = &self
            .conn
            .query_all(sea_orm::Statement::from_string(
                DatabaseBackend::Sqlite,
                format!(
                    r#"
        SELECT * FROM anime
        WHERE json_extract(anime_info, '$.alternative_titles') LIKE %{}%
        "#,
                    name
                ),
            ))
            .await?;
        let mut res = Vec::new();
        for i in r {
            res.push(
                anime::Model {
                    id: i.try_get("", "id")?,
                    status: i.try_get("", "status")?,
                    rule_name: i.try_get("", "rule_name")?,
                    is_search: i.try_get("", "is_search")?,
                    is_lock: i.try_get("", "is_lock")?,
                    progress: i.try_get("", "progress")?,
                    anime_info: i.try_get("", "anime_info")?,
                }
                .into(),
            );
        }
        Ok(Some(res))
    }

    async fn set_anime_recode(
        &self,
        anime_id: i64,
        anime_rss_record: model::rss::AnimeRssRecord,
    ) -> std::result::Result<(), anyhow::Error> {
        anime_record::ActiveModel {
            title: Set(anime_rss_record.title),
            anime_id: Set(anime_id),
            magnet: Set(anime_rss_record.magnet),
            rule_name: Set(anime_rss_record.rule_name),
            info_hash: Set(anime_rss_record.info_hash),
        }
        .insert(&self.conn)
        .await?;
        Ok(())
    }

    async fn get_anime_record(
        &self,
        anime_id: i64,
        info_hash: &str,
    ) -> std::result::Result<Option<model::rss::AnimeRssRecord>, anyhow::Error> {
        if let Some(r) = anime_record::Entity::find()
            .filter(anime_record::Column::AnimeId.eq(anime_id))
            .filter(anime_record::Column::InfoHash.eq(info_hash))
            .one(&self.conn)
            .await?
        {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }

    async fn get_anime_rss_recodes(
        &self,
        anime_id: i64,
    ) -> std::result::Result<Option<Vec<model::rss::AnimeRssRecord>>, anyhow::Error> {
        let r = anime_record::Entity::find()
            .filter(anime_record::Column::AnimeId.eq(anime_id))
            .all(&self.conn)
            .await?;
        if r.is_empty() {
            Ok(None)
        } else {
            Ok(Some(r.into_iter().map(|i| i.into()).collect()))
        }
    }
}

#[async_trait]
impl Rules for SeaDB {
    async fn set_rule(&self, rule: model::rule::Rule) -> std::result::Result<(), anyhow::Error> {
        let tx = self.conn.begin().await?;
        if let Some(r) = rule::Entity::find_by_id(&rule.name).one(&self.conn).await? {
            let mut ar: rule::ActiveModel = r.clone().into();
            ar.re = Set(rule.re);
            ar.cost = Set(rule.cost as u32);
            ar.clone().update(&tx).await?;
        } else {
            rule::ActiveModel {
                name: Set(rule.name),
                re: Set(rule.re),
                cost: Set(rule.cost as u32),
            }
            .insert(&tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn del_rule(&self, name: String) -> std::result::Result<(), anyhow::Error> {
        rule::Entity::delete_by_id(name).exec(&self.conn).await?;
        Ok(())
    }

    async fn get_rule(
        &self,
        name: String,
    ) -> std::result::Result<Option<model::rule::Rule>, anyhow::Error> {
        let r = rule::Entity::find_by_id(name).one(&self.conn).await?;
        if let Some(r) = r {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }

    async fn get_all_rules(
        &self,
    ) -> std::result::Result<Option<Vec<model::rule::Rule>>, anyhow::Error> {
        Ok(Some(
            rule::Entity::find()
                .all(&self.conn)
                .await?
                .into_iter()
                .map(|r| r.into())
                .collect(),
        ))
    }
}

#[async_trait]
impl Rss for SeaDB {
    async fn set_rss(
        &self,
        rss: model::rss::RSSReq,
    ) -> std::result::Result<model::rss::RSS, anyhow::Error> {
        if rss.title.is_none() {
            return Err(Error::msg("cannot set empty title rss"));
        }
        let tx = self.conn.begin().await?;
        let rss_model;
        if let Some(id) = rss.id {
            if let Some(r) = rss::Entity::find_by_id(id).one(&self.conn).await? {
                let mut ar: rss::ActiveModel = r.clone().into();
                ar.search_url = Set(rss.search_url);
                ar.title = Set(rss.title.unwrap_or(r.title));
                ar.url = Set(rss.url);
                ar.clone().update(&tx).await?;
                rss_model = ar.try_into_model()?;
            } else {
                // rss_model = rss::ActiveModel {
                //     id: Set(Uuid::new_v4().to_string()),
                //     url: Set(rss.url),
                //     title: Set(rss.title.unwrap()),
                //     search_url: Set(rss.search_url),
                // }
                rss_model = rss::Model {
                    id: Uuid::new_v4().to_string(),
                    url: rss.url,
                    title: rss.title.unwrap(),
                    search_url: rss.search_url,
                }
                .into_active_model()
                .insert(&tx)
                .await?
                .try_into_model()?;
            }
        } else {
            rss_model = rss::Model {
                id: Uuid::new_v4().to_string(),
                url: rss.url,
                title: rss.title.unwrap(),
                search_url: rss.search_url,
            }
            .into_active_model()
            .insert(&tx)
            .await?
            .try_into_model()?;
        }

        tx.commit().await?;
        Ok(rss_model.into())
    }

    async fn del_rss(&self, id: String) -> std::result::Result<(), anyhow::Error> {
        rss::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    async fn get_rss(
        &self,
        id: String,
    ) -> std::result::Result<Option<model::rss::RSS>, anyhow::Error> {
        if let Some(r) = rss::Entity::find_by_id(id).one(&self.conn).await? {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }

    async fn get_all_rss(
        &self,
    ) -> std::result::Result<Option<Vec<model::rss::RSS>>, anyhow::Error> {
        Ok(Some(
            rss::Entity::find()
                .all(&self.conn)
                .await?
                .into_iter()
                .map(|r| r.into())
                .collect(),
        ))
    }
}

#[async_trait]
impl User for SeaDB {
    async fn update_user(
        &self,
        user: model::user::UserEntity,
    ) -> std::result::Result<(), anyhow::Error> {
        if let Some(u) = user::Entity::find_by_id(user.id).one(&self.conn).await? {
            let mut au: user::ActiveModel = u.into();
            au.username = Set(user.username);
            au.chatacter = Set(user.chatacter.into());
            au.update(&self.conn).await?;
        }
        Ok(())
    }

    async fn create_user(
        &self,
        user: model::user::UserEntity,
    ) -> std::result::Result<model::user::UserEntity, anyhow::Error> {
        if (user::Entity::find()
            .filter(user::Column::Username.eq(&user.username))
            .one(&self.conn)
            .await?)
            .is_some()
        {
            Err(Error::msg("user alreay exist"))
        } else {
            Ok(user::ActiveModel {
                id: Set(0),
                username: Set(user.username),
                password: Set(user.password),
                chatacter: Set(user.chatacter.into()),
            }
            .insert(&self.conn)
            .await?
            .try_into_model()?
            .into())
        }
    }

    async fn get_user(
        &self,
        id: i64,
    ) -> std::result::Result<Option<model::user::UserEntity>, anyhow::Error> {
        if let Some(u) = user::Entity::find_by_id(id).one(&self.conn).await? {
            Ok(Some(u.into()))
        } else {
            Ok(None)
        }
    }

    async fn get_user_from_username(
        &self,
        username: &str,
    ) -> std::result::Result<Option<model::user::UserEntity>, anyhow::Error> {
        if let Some(r) = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(&self.conn)
            .await?
        {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }

    async fn get_users(
        &self,
    ) -> std::result::Result<Option<Vec<model::user::UserEntity>>, anyhow::Error> {
        Ok(Some(
            user::Entity::find()
                .all(&self.conn)
                .await?
                .into_iter()
                .map(|r| r.into())
                .collect(),
        ))
    }

    async fn edit_password(&self, id: i64, password: &str) -> anyhow::Result<()> {
        if let Some(u) = user::Entity::find_by_id(id).one(&self.conn).await? {
            let mut au: user::ActiveModel = u.into();
            au.password = Set(UserEntity::into_sha256_pwd(password.to_string()));
            au.update(&self.conn).await?;
        }
        Ok(())
    }

    async fn set_register_code(
        &self,
        register: model::user::RegisterCode,
    ) -> std::result::Result<(), anyhow::Error> {
        let now = Local::now().to_utc().timestamp();
        _ = register_code::Entity::delete_many()
            .filter(
                Condition::any()
                    .add(register_code::Column::Timers.lte(0))
                    .add(
                        Expr::col(register_code::Column::Now)
                            .add(Expr::col(register_code::Column::Expire))
                            .gt(now),
                    ),
            )
            .exec(&self.conn)
            .await;
        register_code::ActiveModel {
            id: Set(0),
            timers: Set(register.timers as u32),
            expire: Set(register.expire),
            now: Set(register.now),
            code: Set(register.code),
        }
        .insert(&self.conn)
        .await?;
        Ok(())
    }

    async fn get_register_code(
        &self,
        code: String,
    ) -> std::result::Result<Option<model::user::RegisterCode>, anyhow::Error> {
        let now = Local::now().to_utc().timestamp();
        _ = register_code::Entity::delete_many()
            .filter(
                Condition::any()
                    .add(register_code::Column::Timers.lte(0))
                    .add(
                        Expr::col(register_code::Column::Now)
                            .add(Expr::col(register_code::Column::Expire))
                            .gt(now),
                    ),
            )
            .exec(&self.conn)
            .await;
        if let Some(r) = register_code::Entity::find()
            .filter(register_code::Column::Code.eq(code))
            .one(&self.conn)
            .await?
        {
            Ok(Some(r.into()))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl ServiceConfig for SeaDB {
    async fn set_path(&self, path: &str) -> Result<(), anyhow::Error> {
        let tx = self.conn.begin().await?;
        if let Some(r) = config::Entity::find()
            .filter(config::Column::Key.eq(config::ConfigKey::DownloadPath))
            .one(&tx)
            .await?
        {
            let mut r: config::ActiveModel = r.into();
            r.value = Set(path.to_string());
            r.update(&tx).await?;
        } else {
            config::ActiveModel {
                key: Set(config::ConfigKey::DownloadPath),
                value: Set(path.to_string()),
            }
            .insert(&tx)
            .await?;
        }
        Ok(tx.commit().await?)
    }

    async fn get_path(&self) -> Result<Option<String>, anyhow::Error> {
        if let Some(r) = config::Entity::find_by_id(config::ConfigKey::DownloadPath)
            .one(&self.conn)
            .await?
        {
            Ok(Some(r.value))
        } else {
            Ok(None)
        }
    }

    async fn set_qbit(
        &self,
        url: &str,
        username: &str,
        password: &str,
    ) -> Result<(), anyhow::Error> {
        let tx = self.conn.begin().await?;
        if let Some(r) = config::Entity::find()
            .filter(config::Column::Key.eq(config::ConfigKey::QbitConfig))
            .one(&tx)
            .await?
        {
            let mut r: config::ActiveModel = r.into();
            r.value = Set(serde_json::to_string(&QbitConfig {
                url: url.to_string(),
                username: username.to_string(),
                password: password.to_string(),
            })?);
            r.update(&tx).await?;
        } else {
            config::ActiveModel {
                key: Set(config::ConfigKey::QbitConfig),
                value: Set(serde_json::to_string(&QbitConfig {
                    url: url.to_string(),
                    username: username.to_string(),
                    password: password.to_string(),
                })?),
            }
            .insert(&tx)
            .await?;
        }
        Ok(tx.commit().await?)
    }

    async fn get_qbit(&self) -> Result<Option<anna::qbit::qbitorrent::QbitConfig>, anyhow::Error> {
        if let Some(r) = config::Entity::find_by_id(config::ConfigKey::QbitConfig)
            .one(&self.conn)
            .await?
        {
            Ok(Some(serde_json::from_str(&r.value)?))
        } else {
            Ok(None)
        }
    }
}
