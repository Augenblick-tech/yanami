use anna::{anime::tracker::AnimeInfo, qbit::qbitorrent::QbitConfig};
use anyhow::{Error, Result};
use async_trait::async_trait;
use entity::{anime, anime_record, config, register_code, rss, rule, user};
use model::{
    anime::AnimeStatus,
    rss::{AnimeRssRecord, RSSReq, RSS},
    rule::Rule,
    user::{RegisterCode, UserEntity},
};
use provider::db::{Anime, Db, Rss, Rules, ServiceConfig, User};
use sqlx::{query, query_as, sqlite::SqlitePoolOptions, Acquire, Pool, Sqlite};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxDB {
    conn: Pool<Sqlite>,
}

impl SqlxDB {
    pub async fn new(s: &str) -> Result<Self> {
        let conn = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(s)
            .await?;
        Ok(Self { conn })
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
                  "title" varchar NOT NULL PRIMARY KEY, "anime_id" integer NOT NULL, "magnet" varchar NOT NULL, "rule_name" varchar NOT NULL, "info_hash" varchar NOT NULL 
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
                query("UPDATE anime SET anime_info = $1 WHERE id = $2")
                    .bind(serde_json::to_string(i)?)
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
    async fn search_calender(&self, name: String) -> Result<Option<Vec<AnimeStatus>>, Error> {
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
}
