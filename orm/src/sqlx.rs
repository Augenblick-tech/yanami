use anna::{anime::tracker::AnimeInfo, qbit::qbitorrent::QbitConfig};
use anyhow::{Error, Result};
use async_trait::async_trait;

use entity::{
    anime, anime_record, config, register_code,
    rss::{self, RssRecordModel},
    rule, user,
};
use model::{
    anime::{AnimeStatus, AnimesQuertOption},
    rss::{AnimeRssRecord, RSSReq, RssRecord, RSS},
    rule::Rule,
    user::{RegisterCode, UserEntity},
};
use provider::db::{Anime, Db, Rss, Rules, ServiceConfig, User};
use sqlx::{query, query_as, sqlite::SqlitePoolOptions, Acquire, Pool, Sqlite};
use uuid::Uuid;

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

#[derive(Clone)]
pub struct SqlxDB {
    conn: Pool<Sqlite>,
    write_lock: Arc<TokioMutex<()>>,
}

impl SqlxDB {
    pub async fn new(s: &str) -> Result<Self> {
        let conn = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(s)
            .await?;
        Ok(Self {
            conn,
            write_lock: Arc::new(TokioMutex::new(())),
        })
    }

    async fn up(&self) -> Result<()> {
        // CREATE TABLE IF NOT EXISTS "user" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "username" varchar NOT NULL, "password" varchar NOT NULL, "chatacter" varchar NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "user" (
                 "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                 "username" VARCHAR NOT NULL,
                 "password" VARCHAR NOT NULL,
                 "chatacter" VARCHAR NOT NULL
                );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE IF NOT EXISTS "rule" ( "name" varchar NOT NULL PRIMARY KEY, "re" varchar NOT NULL, "cost" integer NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "rule" (
                  "name" varchar NOT NULL PRIMARY KEY, "re" varchar NOT NULL, "cost" integer NOT NULL 
                 );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE "IF NOT EXISTS rss" ( "id" varchar NOT NULL PRIMARY KEY, "url" varchar NULL, "title" varchar NOT NULL, "search_url" varchar NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "rss" (
                 "id" varchar NOT NULL PRIMARY KEY, "url" varchar NULL, "title" varchar NOT NULL, "search_url" varchar NULL 
                 );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE IF NOT EXISTS "register" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "code" varchar NOT NULL, "timers" integer NOT NULL, "expire" integer NOT NULL, "now" integer NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "register" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "code" varchar NOT NULL, "timers" integer NOT NULL, "expire" integer NOT NULL, "now" integer NOT NULL
                  );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE IF NOT EXISTS "config" ( "key" varchar NOT NULL PRIMARY KEY, "value" varchar NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "config" (
                  "key" varchar NOT NULL PRIMARY KEY, "value" varchar NOT NULL 
                 );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE IF NOT EXISTS "anime" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "status" boolean NOT NULL, "is_lock" boolean NOT NULL, "is_search" boolean NOT NULL, "progress" integer NOT NULL, "anime_info" json_text NOT NULL, "rule_name" varchar NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "anime" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "status" boolean NOT NULL, "is_lock" boolean NOT NULL, "is_search" boolean NOT NULL, "progress" integer NOT NULL, "anime_info" json_text NOT NULL, "rule_name" varchar NOT NULL 
                 );"#,
        )
        .execute(&self.conn)
        .await?;
        // CREATE TABLE IF NOT EXISTS "anime_record" ( "title" varchar NOT NULL PRIMARY KEY, "anime_id" integer NOT NULL, "magnet" varchar NOT NULL, "rule_name" varchar NOT NULL, "info_hash" varchar NOT NULL )
        query(
            r#"CREATE TABLE IF NOT EXISTS "anime_record" (
                  "title" varchar NOT NULL PRIMARY KEY, "anime_id" integer NOT NULL, "magnet" varchar NOT NULL, "rule_name" varchar NOT NULL, "info_hash" varchar NOT NULL, "created_time" integer NOT NULL DEFAULT (strftime('%s', 'now'))
                 );"#,
        )
        .execute(&self.conn)
        .await?;

        // CREATE TABLE IF NOT EXISTS "rss_record" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "title" varchar NOT NULL, "magnet" varchar NOT NULL, "info_hash" varchar NOT NULL, "created_time" integer, "source" varchar, "info" json_text, "url" varchar )
        query(
                    r#"CREATE TABLE IF NOT EXISTS "rss_record" (
                          "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, "title" VARCHAR NOT NULL, "magnet" VARCHAR NOT NULL, "info_hash" VARCHAR NOT NULL UNIQUE, "created_time" INTEGER NOT NULL DEFAULT (strftime('%s', 'now')), "source" VARCHAR, "info" JSON_TEXT, "url" VARCHAR
                         );"#,
                )
                .execute(&self.conn)
                .await?;
        Ok(())
    }
}

impl SqlxDB {
    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        if query("SELECT * FROM config WHERE key = $1")
            .bind(key)
            .fetch_optional(&mut *t)
            .await?
            .is_some()
        {
            query("UPDATE config SET value = $1 WHERE key = $2")
                .bind(value)
                .bind(key)
                .execute(&mut *t)
                .await?;
        } else {
            query("INSERT INTO config (key, value) VALUES ($1, $2)")
                .bind(key)
                .bind(value)
                .execute(&mut *t)
                .await?;
        }
        t.commit().await?;
        Ok(())
    }

    async fn get_config(&self, key: String) -> Result<Option<String>> {
        let m = query_as::<_, (String,)>("SELECT value FROM config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.conn)
            .await?;

        Ok(m.map(|(v,)| v))
    }
}

#[async_trait]
impl Db for SqlxDB {
    async fn is_empty(&self) -> Result<bool, Error> {
        self.up().await?;
        if query("SELECT * FROM user WHERE chatacter = 'admin' LIMIT 1")
            .fetch_optional(&self.conn)
            .await?
            .is_none()
        {
            query("INSERT INTO sqlite_sequence (name,seq) SELECT 'user', 10000 WHERE NOT EXISTS (SELECT changes() AS change FROM sqlite_sequence WHERE change <> 0);").execute(&self.conn).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl User for SqlxDB {
    async fn update_user(&self, user: UserEntity) -> Result<(), Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.begin().await?;

        query_as::<_, user::Model>("SELECT * FROM user WHERE id = $1 LIMIT 1")
            .bind(user.id)
            .fetch_one(&mut *t)
            .await?;
        let c: String = user.chatacter.into();
        query("UPDATE user SET password = $1, chatacter = $2 WHERE id = $3 ")
            .bind(c)
            .bind(user.id)
            .execute(&mut *t)
            .await?;
        t.commit().await?;
        Ok(())
    }
    async fn create_user(&self, user: UserEntity) -> Result<UserEntity, Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        let c: String = user.chatacter.into();
        let u = query_as::<_, user::Model>(
            "INSERT INTO user (username, password, chatacter) VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(user.username)
        .bind(user.password)
        .bind(c)
        .fetch_one(&mut *t)
        .await?;
        t.commit().await?;
        Ok(u.into())
    }
    async fn get_user(&self, id: i64) -> Result<Option<UserEntity>, Error> {
        if let Some(m) = query_as::<_, user::Model>("SELECT * FROM user WHERE id = $1 LIMIT 1")
            .bind(id)
            .fetch_optional(&self.conn)
            .await?
        {
            Ok(Some(m.into()))
        } else {
            Ok(None)
        }
    }
    async fn get_user_from_username(&self, username: &str) -> Result<Option<UserEntity>, Error> {
        if let Some(m) =
            query_as::<_, user::Model>("SELECT * FROM user WHERE username = $1 LIMIT 1")
                .bind(username)
                .fetch_optional(&self.conn)
                .await?
        {
            Ok(Some(m.into()))
        } else {
            Ok(None)
        }
    }
    async fn get_users(&self) -> Result<Option<Vec<UserEntity>>, Error> {
        let vm = query_as::<_, user::Model>("SELECT * FROM user")
            .fetch_all(&self.conn)
            .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }
    async fn edit_password(&self, id: i64, password: &str) -> anyhow::Result<()> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.begin().await?;

        query_as::<_, user::Model>("SELECT * FROM user WHERE id = $1 LIMIT 1")
            .bind(id)
            .fetch_one(&mut *t)
            .await?;
        query("UPDATE user SET password = $1 WHERE id = $2 ")
            .bind(password)
            .bind(id)
            .execute(&mut *t)
            .await?;
        t.commit().await?;
        Ok(())
    }

    async fn set_register_code(&self, registry: RegisterCode) -> Result<(), Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.begin().await?;
        query("DELETE FROM register WHERE now + expire > strftime('%s', 'now') AND timers <= 0")
            .execute(&mut *t)
            .await?;
        query("INSERT INTO (code, now, expire, timers) VALUES ($1, $2, $3, $4)")
            .bind(registry.code)
            .bind(registry.now)
            .bind(registry.expire)
            .bind(registry.timers as u32)
            .execute(&mut *t)
            .await?;
        t.commit().await?;
        Ok(())
    }
    async fn get_register_code(&self, code: String) -> Result<Option<RegisterCode>, Error> {
        Ok(
            query_as::<_, register_code::Model>("SELECT * FROM register WHERE code = $1 LIMIT 1")
                .bind(&code)
                .fetch_optional(&self.conn)
                .await?
                .map(|m| m.into()),
        )
    }
}

#[async_trait]
impl Rss for SqlxDB {
    async fn set_rss(&self, rss: RSSReq) -> Result<RSS, Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        if let Some(id) = rss.id {
            if (query_as::<_, rss::Model>("SELECT * FROM rss WHERE id = $1 LIMIT 1")
                .bind(&id)
                .fetch_optional(&mut *t)
                .await?)
                .is_some()
            {
                let m = query_as::<_, rss::Model>("UPDATE rss SET title = $1, url = $2, search_url = $3 WHERE id = $4 RETURNING *")
                    .bind(rss.title)
                    .bind(rss.url)
                    .bind(rss.search_url)
                    .bind(&id)
                    .fetch_one(&mut *t).await?;
                t.commit().await?;
                return Ok(m.into());
            }
        }
        let m = query_as::<_, rss::Model>(
            "INSERT INTO rss (id, url, title, search_url) VALUES ($1, $2, $3, $4) RETURNING *",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(rss.url)
        .bind(rss.title)
        .bind(rss.search_url)
        .fetch_one(&mut *t)
        .await?;
        t.commit().await?;
        Ok(m.into())
    }
    async fn del_rss(&self, id: String) -> Result<(), Error> {
        query("DELETE FROM rss WHERE id = $1")
            .bind(id)
            .execute(&self.conn)
            .await?;
        Ok(())
    }
    async fn get_rss(&self, id: String) -> Result<Option<RSS>, Error> {
        if let Some(m) = query_as::<_, rss::Model>("SELECT * FROM rss WHERE id = $1 LIMIT 1")
            .bind(&id)
            .fetch_optional(&self.conn)
            .await?
        {
            Ok(Some(m.into()))
        } else {
            Ok(None)
        }
    }
    async fn get_all_rss(&self) -> Result<Option<Vec<RSS>>, Error> {
        let vm = query_as::<_, rss::Model>("SELECT * FROM rss")
            .fetch_all(&self.conn)
            .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }

    async fn insert_or_update_rss_record(&self, record: &RssRecord) -> Result<(), Error> {
        let _guard = self.write_lock.lock().await;
        let created_time = record
            .pub_date
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        if let Some(mut m) =
            query_as::<_, RssRecordModel>("SELECT * FROM rss_record WHERE info_hash = $1 LIMIT 1")
                .bind(&record.info_hash)
                .fetch_optional(&self.conn)
                .await?
        {
            tracing::debug!("RSS record with hash {} already exists.", &record.info_hash,);
            if m.info.is_none() {
                if record.info.is_none() {
                    return Ok(());
                }
                tracing::debug!(
                    "Updating existing RSS record info for hash: {}",
                    &record.info_hash
                );
                m.info = record.info.clone();
                query("UPDATE rss_record SET info = $1, url = $2 WHERE info_hash = $3")
                    .bind(&m.info)
                    .bind(&record.url)
                    .bind(&m.info_hash)
                    .execute(&self.conn)
                    .await?;
                tracing::info!(
                    "Updated existing RSS record info for hash: {}",
                    &record.info_hash
                );
                return Ok(());
            } else {
                tracing::debug!(
                    "RSS record already exists and has info, skipping update. Hash: {}",
                    &record.info_hash
                );
            }
        } else {
            tracing::debug!(
                "Inserting new RSS record. Hash: {}, Title: {}",
                &record.info_hash,
                &record.title
            );
            query("INSERT INTO rss_record (title, magnet, info_hash, created_time, source, info, url) VALUES ($1, $2, $3, $4, $5, $6, $7)")
                .bind(&record.title)
                .bind(&record.magnet)
                .bind(&record.info_hash)
                .bind(created_time)
                .bind(&record.source)
                .bind(&record.info)
                .bind(&record.url)
                .execute(&self.conn)
                .await?;
            tracing::info!(
                "Inserted new RSS record. Hash: {}, Title: {}",
                &record.info_hash,
                &record.title
            );
        }
        Ok(())
    }

    async fn select_latest_rss_records(&self) -> Result<Vec<RssRecord>, Error> {
        // TODO: 获取最近三个小时的所有记录返回
        let m = query_as::<_, RssRecordModel>(
            "SELECT * FROM rss_record WHERE created_time >= (strftime('%s', 'now') - 3 * 3600) ORDER BY created_time DESC",
        )
        .fetch_all(&self.conn)
        .await?;

        Ok(m.into_iter().map(|model| model.into()).collect())
    }

    async fn get_rss_record_by_url(&self, url: &str) -> Result<Option<RssRecord>, Error> {
        let m = query_as::<_, RssRecordModel>("SELECT * FROM rss_record WHERE url = $1 LIMIT 1")
            .bind(url)
            .fetch_optional(&self.conn)
            .await?;
        Ok(m.map(|model| model.into()))
    }

    async fn search_rss_records_by_keywords(
        &self,
        keywords: &[String],
    ) -> Result<Vec<RssRecord>, Error> {
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let mut query_string = String::from("SELECT * FROM rss_record WHERE ");
        let mut conditions = Vec::new();

        for (i, _) in keywords.iter().enumerate() {
            conditions.push(format!("title LIKE ${}", i + 1));
        }

        query_string.push_str(&conditions.join(" OR "));
        query_string.push_str(" ORDER BY created_time DESC");

        let mut query_builder = query_as::<_, RssRecordModel>(&query_string);

        for keyword in keywords {
            query_builder = query_builder.bind(format!("%{}%", keyword));
        }

        let m = query_builder.fetch_all(&self.conn).await?;

        Ok(m.into_iter().map(|model| model.into()).collect())
    }
}

#[async_trait]
impl ServiceConfig for SqlxDB {
    async fn set_path(&self, path: &str) -> Result<(), Error> {
        let key: String = config::ConfigKey::DownloadPath.into();
        self.set_config(&key, path).await
    }

    async fn get_path(&self) -> Result<Option<String>, Error> {
        self.get_config(config::ConfigKey::DownloadPath.into())
            .await
    }

    async fn set_qbit(&self, url: &str, username: &str, password: &str) -> Result<(), Error> {
        let key: String = config::ConfigKey::QbitConfig.into();
        let value = serde_json::to_string(&QbitConfig {
            url: url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        })?;
        self.set_config(&key, &value).await
    }

    async fn get_qbit(&self) -> Result<Option<QbitConfig>, Error> {
        Ok(self
            .get_config(config::ConfigKey::QbitConfig.into())
            .await?
            .and_then(|v| serde_json::from_str(&v).ok()))
    }
}

#[async_trait]
impl Rules for SqlxDB {
    async fn set_rule(&self, rule: Rule) -> Result<(), Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        query("INSERT INTO rule (name, cost, re) VALUES ($1, $2, $3)")
            .bind(rule.name)
            .bind(rule.cost as u32)
            .bind(rule.re)
            .execute(&mut *t)
            .await?;
        t.commit().await?;
        Ok(())
    }
    async fn del_rule(&self, name: String) -> Result<(), Error> {
        query("DELETE FROM rule WHERE name = $1")
            .bind(name)
            .execute(&self.conn)
            .await?;
        Ok(())
    }
    async fn get_rule(&self, name: String) -> Result<Option<Rule>, Error> {
        Ok(
            query_as::<_, rule::Model>("SELECT * FROM rule WHERE name = $1 LIMIT 1")
                .bind(&name)
                .fetch_optional(&self.conn)
                .await?
                .map(|m| m.into()),
        )
    }
    async fn get_all_rules(&self) -> Result<Option<Vec<Rule>>, Error> {
        let vm = query_as::<_, rule::Model>("SELECT * FROM rule")
            .fetch_all(&self.conn)
            .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }
}

#[async_trait]
impl Anime for SqlxDB {
    // 覆盖所有存在id的信息，不存在id的则创建，如果is_lock为true这跳过覆盖
    async fn set_calenders(&self, calender: Vec<AnimeInfo>) -> Result<(), Error> {
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        for i in calender.iter() {
            if let Some(m) =
                query_as::<_, anime::Model>("SELECT * FROM anime WHERE id = $1 LIMIT 1")
                    .bind(i.id)
                    .fetch_optional(&mut *t)
                    .await?
            {
                if m.is_lock {
                    continue;
                }
                // 检查状态，如果更新的集数变大了，则需要将状态放开
                let old_record = AnimeStatus::from(m);
                let status = if !old_record.status && old_record.anime_info.eps < i.eps {
                    true
                } else {
                    old_record.status
                };
                query("UPDATE anime SET anime_info = $1, status = $2 WHERE id = $3")
                    .bind(serde_json::to_string(i)?)
                    .bind(status)
                    .bind(i.id)
                    .execute(&mut *t)
                    .await?;
            } else {
                query(
                    "INSERT INTO anime (id, is_lock, is_search, status, rule_name, anime_info, progress) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(i.id)
                .bind(false)
                .bind(false)
                .bind(true)
                .bind("")
                .bind(serde_json::to_string(i)?)
                .bind(0)
                .execute(&mut *t)
                .await?;
            }
        }
        t.commit().await?;
        Ok(())
    }

    // 忽略is_lock
    async fn set_calender(&self, anime_status: AnimeStatus) -> Result<(), Error> {
        let _guard = self.write_lock.lock().await;
        let mut conn = self.conn.acquire().await?;
        let mut t = conn.acquire().await?.begin().await?;
        if (query("SELECT * FROM anime WHERE id = $1 LIMIT 1")
            .bind(anime_status.anime_info.id)
            .fetch_one(&mut *t)
            .await)
            .is_ok()
        {
            query("UPDATE anime SET anime_info = $1, is_lock = $2, is_search = $3, status = $4, rule_name = $5, progress = $6 WHERE id = $7")
                .bind(serde_json::to_string(&anime_status.anime_info)?)
                .bind(anime_status.is_lock)
                .bind(anime_status.is_search)
                .bind(anime_status.status)
                .bind(anime_status.rule_name)
                .bind(anime_status.progress as i64)
                .bind(anime_status.anime_info.id)
                .execute(&mut *t)
                .await?;
        } else {
            query("INSERT INTO anime (anime_info, is_lock, is_search, status, rule_name, progress, id) VALUES ($1, $2, $3, $4, $5, $6, $7)")
                .bind(serde_json::to_string(&anime_status.anime_info)?)
                .bind(anime_status.is_lock)
                .bind(anime_status.is_search)
                .bind(anime_status.status)
                .bind(anime_status.rule_name)
                .bind(anime_status.progress as i64)
                .bind(anime_status.anime_info.id)
                .execute(&mut *t)
                .await?;
        }
        t.commit().await?;
        Ok(())
    }
    async fn get_calenders(&self) -> Result<Option<Vec<AnimeStatus>>, Error> {
        let vm = query_as::<_, anime::Model>("SELECT * FROM anime")
            .fetch_all(&self.conn)
            .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }
    async fn get_calender(&self, id: i64) -> Result<Option<AnimeStatus>, Error> {
        Ok(
            query_as::<_, anime::Model>("SELECT * FROM anime WHERE id = $1 LIMIT 1")
                .bind(id)
                .fetch_optional(&self.conn)
                .await?
                .map(|m| m.into()),
        )
    }

    async fn get_calenders_with_query(
        &self,
        option: Option<AnimesQuertOption>,
    ) -> Result<Vec<AnimeStatus>, Error> {
        if option.is_none() {
            if let Some(r) = self.get_calenders().await? {
                return Ok(r);
            } else {
                return Ok(vec![]);
            }
        }
        let query_options = option.unwrap();

        if query_options.enable.is_none()
            && query_options.search.is_none()
            && query_options.status.is_none()
            && query_options.name.is_none()
        {
            if let Some(r) = self.get_calenders().await? {
                return Ok(r);
            } else {
                return Ok(vec![]);
            }
        }

        let mut query_string = String::from("SELECT * FROM anime WHERE 1=1");
        let mut param_index = 0; // To track the index for $N parameters

        // Variables to hold values for binding, in the order they will appear in the query.
        let mut enable_val_bind: Option<bool> = None;
        let mut search_val_bind: Option<bool> = None;
        let mut name_val_bind = None;

        // Conditionally build the WHERE clause and collect values for binding.
        // The order of these `if let` blocks determines the order of `$N` placeholders
        // in `query_string` and thus the order for subsequent `bind` calls.

        // 1. Handle `enable` condition: Maps to `status` boolean in DB.
        if let Some(enable_val) = query_options.enable {
            param_index += 1;
            query_string.push_str(&format!(" AND status = ${}", param_index));
            enable_val_bind = Some(enable_val);
        }

        // 2. Handle `search` condition: Maps to `is_search` boolean in DB.
        if let Some(search_val) = query_options.search {
            param_index += 1;
            query_string.push_str(&format!(" AND is_search = ${}", param_index));
            search_val_bind = Some(search_val);
        }

        // 增加名字模糊搜索
        if let Some(name_val) = query_options.name {
            param_index += 1;
            query_string.push_str(&format!(
                " AND json_extract(anime_info, '$.alternative_titles') LIKE ${}",
                param_index
            ));
            name_val_bind = Some(name_val);
        }

        // 3. Handle `status` condition: Maps to `progress` integer and `status` boolean in DB.
        if let Some(status_option_val) = query_options.status {
            match status_option_val {
                0 => {
                    query_string.push_str(" AND progress = 0");
                }
                1 => {
                    query_string.push_str(
                        " AND progress > 0 AND progress < json_extract(anime_info, '$.eps')",
                    );
                }
                2 => {
                    query_string.push_str(" AND progress >= json_extract(anime_info, '$.eps')");
                }
                _ => {
                    // Ignore unsupported status values as per requirement (do not limit this item)
                }
            }
        }

        // Initialize the query builder with the dynamically constructed SQL string.
        let mut query_builder = sqlx::query_as::<_, anime::Model>(&query_string);

        // Bind parameters in the exact order they were appended to the query string.
        // This is crucial for macro-free `sqlx::query_as`.

        if let Some(val) = enable_val_bind {
            query_builder = query_builder.bind(val);
        }
        if let Some(val) = search_val_bind {
            query_builder = query_builder.bind(val);
        }
        if let Some(val) = name_val_bind {
            query_builder = query_builder.bind(format!("%{val}%"));
        }

        // Execute the query.
        let anime_models = query_builder.fetch_all(&self.conn).await?;

        // Convert `AnimeModel` instances to `AnimeStatus` and return.
        if anime_models.is_empty() {
            Ok(vec![])
        } else {
            Ok(anime_models.into_iter().map(|model| model.into()).collect())
        }
    }

    async fn search_calender(
        &self,
        name: String,
        _option: Option<AnimesQuertOption>,
    ) -> Result<Option<Vec<AnimeStatus>>, Error> {
        let vm = query_as::<_, anime::Model>(
            "SELECT * FROM anime WHERE json_extract(anime_info, '$.alternative_titles') LIKE $1",
        )
        .bind(format!("%{name}%"))
        .fetch_all(&self.conn)
        .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }

    async fn set_anime_recode(
        &self,
        anime_id: i64,
        anime_rss_record: AnimeRssRecord,
    ) -> Result<(), Error> {
        let _guard = self.write_lock.lock().await;
        query("INSERT INTO anime_record (anime_id, title, magnet, rule_name, info_hash) VALUES ($1, $2, $3, $4, $5)")
            .bind(anime_id)
            .bind(&anime_rss_record.title)
            .bind(&anime_rss_record.magnet)
            .bind(&anime_rss_record.rule_name)
            .bind(&anime_rss_record.info_hash)
            .execute(&self.conn).await?;
        Ok(())
    }
    async fn get_anime_record(
        &self,
        anime_id: i64,
        info_hash: &str,
    ) -> Result<Option<AnimeRssRecord>, Error> {
        Ok(query_as::<_, anime_record::Model>(
            "select * from anime_record where anime_id = $1 and info_hash = $2 limit 1",
        )
        .bind(anime_id)
        .bind(info_hash)
        .fetch_optional(&self.conn)
        .await?
        .map(|m| m.into()))
    }
    async fn get_anime_rss_recodes(
        &self,
        anime_id: i64,
    ) -> Result<Option<Vec<AnimeRssRecord>>, Error> {
        let vm =
            query_as::<_, anime_record::Model>("SELECT * FROM anime_record WHERE anime_id = $1")
                .bind(anime_id)
                .fetch_all(&self.conn)
                .await?;
        if vm.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vm.into_iter().map(|i| i.into()).collect()))
        }
    }

    async fn latest_anime_records(&self, n: i64) -> Result<Vec<AnimeRssRecord>, Error> {
        let vm = query_as::<_, anime_record::Model>(
            "SELECT * FROM anime_record ORDER BY created_time DESC LIMIT $1",
        )
        .bind(n)
        .fetch_all(&self.conn)
        .await?;
        Ok(vm.into_iter().map(|i| i.into()).collect())
    }
}
