use std::{
    any::Any,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use anime::repository::{AnimeRepository, AnimeSnapshot};
use async_trait::async_trait;
use chrono::Datelike;
use domain::{
    anime::{
        AirDate, AnimeId, AnimeListQuery, AnimeMetadata, AnimeMetadataRepository,
        AnimeStateRepository, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount, SeasonNumber,
    },
    feed::{
        FeedSource, FeedSourceId, Resource, ResourceId, ResourceRepository, ResourceSource,
        SpaceFeedRepository,
    },
    rule::{MatchingRule, MatchingRuleId, SpaceRuleRepository},
    shared::{
        biz::{BizContext, BizFactory, InfraTxProvider},
        error::DomainError,
        identifier::IdSequence,
    },
    space::{PersonalSpaceBinding, Space, SpaceId, SpaceRepository},
    subscription::{
        LatestMatchRecord, MatchRecord, MatchRecordRepository, MatchResourceId, PoolSubLink,
        SearchPoolEntry, SearchPoolEntryData, SearchPoolRepository, SubscriptionAnime,
        SubscriptionAnimeRepository, SubscriptionSearchState,
    },
    user::{PasswordHash, User, UserId, UserRepository, UserRole, Username},
};
use serde::{Deserialize, Serialize};
use service::download::runtime::{
    UserDownloadDriverBindingStore, UserQbitDownloadProfile, UserQbitDownloadProfileStore,
};
use service::download::shared::error::ApplicationError;
use service::system::service::SystemInfrastructureInitializer;
use sha1::{Digest, Sha1};
use sqlx::{
    pool::PoolConnection, query, query_as, sqlite::SqlitePoolOptions, Acquire, Pool, QueryBuilder,
    Row, Sqlite,
};
use tokio::sync::Mutex;
use user::gateway::UserIdGenerator;

use crate::secret::SecretProtector;

const LATEST_SCHEMA_VERSION: i64 = 2;
const SCHEMA_VERSION_KEY: &str = "schema_version";

/// 基于 SQLite 的全局数据库实现。
#[derive(Clone)]
pub struct SqliteDb {
    pool: Pool<Sqlite>,
    write_lock: Arc<Mutex<()>>,
    secret_protector: Arc<SecretProtector>,
    user_id_counter: Arc<Mutex<Option<i64>>>,
    subscription_space_id_counter: Arc<Mutex<Option<i64>>>,
    next_biz_id: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct SqliteBizProvider {
    state: Arc<Mutex<SqliteBizState>>,
}

struct SqliteBizState {
    id: u64,
    connection: PoolConnection<Sqlite>,
    committed: bool,
}

impl Drop for SqliteBizState {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        tracing::trace!("biz #{} dropped without commit, auto-rolling back", self.id);
        // Drop 中无法返回错误，回滚失败时已有日志记录，不做额外处理
        if let Err(error) =
            futures_executor::block_on(sqlx::query("ROLLBACK").execute(&mut *self.connection))
        {
            tracing::error!(?error, "biz auto-rollback failed");
        }
    }
}

struct SqliteBizDb {
    biz: SqliteBizProvider,
}

pub(crate) struct SqliteBizUserIds {
    biz: SqliteBizProvider,
    next_id: Mutex<Option<i64>>,
}

struct SqliteBizIdentifiers {
    biz: SqliteBizProvider,
    next_space_id: Mutex<Option<i64>>,
}

impl SqliteBizDb {
    fn new(biz: SqliteBizProvider) -> Self {
        Self { biz }
    }
}

impl SqliteBizUserIds {
    pub(crate) fn new(biz: SqliteBizProvider) -> Self {
        Self {
            biz,
            next_id: Mutex::new(None),
        }
    }
}

impl SqliteBizIdentifiers {
    fn new(biz: SqliteBizProvider) -> Self {
        Self {
            biz,
            next_space_id: Mutex::new(None),
        }
    }
}

#[async_trait]
impl InfraTxProvider for SqliteBizProvider {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    async fn commit(&self) -> Result<(), DomainError> {
        let mut state = self.state.lock().await;
        if state.committed {
            return Err(DomainError::InvariantViolation(
                "biz context already committed",
            ));
        }
        query("COMMIT")
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz context commit failed", error))?;
        state.committed = true;
        tracing::trace!("biz #{} committed", state.id);
        Ok(())
    }

    async fn rollback(&self) -> Result<(), DomainError> {
        let mut state = self.state.lock().await;
        if state.committed {
            return Ok(());
        }
        query("ROLLBACK")
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz context rollback failed", error))?;
        state.committed = true;
        tracing::trace!("biz #{} rolled back", state.id);
        Ok(())
    }
}

impl SqliteDb {
    /// 连接 SQLite 数据库，不执行 schema/migration。
    pub async fn connect(database_url: &str, application_key: &str) -> Result<Self, DomainError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(database_url)
            .await
            .map_err(|error| DomainError::external("anime sqlite connect failed", error))?;
        Ok(Self {
            pool,
            write_lock: Arc::new(Mutex::new(())),
            secret_protector: Arc::new(SecretProtector::new(application_key)?),
            user_id_counter: Arc::new(Mutex::new(None)),
            subscription_space_id_counter: Arc::new(Mutex::new(None)),
            next_biz_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// 创建并初始化一个 SQLite 数据库实例。
    pub async fn new(database_url: &str, application_key: &str) -> Result<Self, DomainError> {
        let database = Self::connect(database_url, application_key).await?;
        let biz = database.open_biz().await?;
        match database.initialize_schema(&biz).await {
            Ok(()) => {
                if let Err(error) = biz.commit().await {
                    // commit 失败后尝试回滚；回滚失败时记录日志但不覆盖原始错误
                    if let Err(rollback_error) = biz.rollback().await {
                        tracing::error!(
                            ?rollback_error,
                            ?error,
                            "rollback after commit failure also failed"
                        );
                    }
                    return Err(error);
                }
            }
            Err(error) => {
                biz.rollback().await?;
                return Err(error);
            }
        }
        Ok(database)
    }

    pub async fn load_personal_space_id(
        &self,
        user_id: domain::user::UserId,
    ) -> Result<Option<domain::space::SpaceId>, DomainError> {
        sqlx::query_as::<_, (i64,)>(
            r#"SELECT personal_space_id FROM "user_space_binding" WHERE user_id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("personal space binding select failed", error))
        .map(|row| row.map(|(space_id,)| domain::space::SpaceId(space_id)))
    }

    fn bind_biz(&self, biz: &BizContext) -> Result<SqliteBizDb, DomainError> {
        Ok(SqliteBizDb::new(self.bind_biz_provider(biz)?))
    }

    pub(crate) fn bind_biz_provider(
        &self,
        biz: &BizContext,
    ) -> Result<SqliteBizProvider, DomainError> {
        let provider = biz
            .provider()
            .as_any()
            .downcast_ref::<SqliteBizProvider>()
            .ok_or(DomainError::InvariantViolation(
                "biz context provider does not match sqlite db",
            ))?;
        Ok(provider.clone())
    }

    pub async fn initialize_schema(&self, biz: &BizContext) -> Result<(), DomainError> {
        let provider = self.bind_biz_provider(biz)?;
        let mut state = provider.state.lock().await;
        let transaction = &mut *state.connection;
        query(
            r#"CREATE TABLE IF NOT EXISTS "user" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
                  "username" varchar NOT NULL,
                  "password" varchar NOT NULL,
                  "chatacter" varchar NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("user table init failed", error))?;

        query(
            r#"CREATE TABLE IF NOT EXISTS "config" (
                  "key" varchar NOT NULL PRIMARY KEY,
                  "value" varchar NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("config table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "anime" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
                  "status" boolean NOT NULL,
                  "is_lock" boolean NOT NULL,
                  "is_search" integer NOT NULL,
                  "progress" integer NOT NULL,
                  "anime_info" json_text NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "anime_subscription" (
                  "user_id" integer NOT NULL,
                  "space_id" integer NOT NULL,
                  "anime_id" integer NOT NULL,
                  "enabled" boolean NOT NULL,
                  "bound_rule_name" text NULL,
                  "search_state" integer NOT NULL DEFAULT 0,
                  "progress" integer NOT NULL,
                  PRIMARY KEY ("user_id", "space_id", "anime_id")
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime_subscription table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "download_record" (
                  "user_id" integer NOT NULL,
                  "space_id" integer NOT NULL,
                  "anime_id" integer NOT NULL,
                  "resource_id" text NOT NULL,
                  "title" text NOT NULL,
                  "source_url" text NOT NULL,
                  "matched_rule_name" text NOT NULL,
                  "published_at" integer NULL,
                  "created_at" integer NOT NULL,
                  PRIMARY KEY ("user_id", "space_id", "anime_id", "resource_id")
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("download_record table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "feed_source" (
                  "id" text NOT NULL PRIMARY KEY,
                  "owner_scope" text NOT NULL,
                  "scope_id" integer NOT NULL,
                  "title" text NOT NULL,
                  "site_url" text NULL,
                  "search_url" text NULL,
                  "source_key" text NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("feed_source table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "matching_rule" (
                  "id" text NOT NULL PRIMARY KEY,
                  "owner_scope" text NOT NULL,
                  "scope_id" integer NOT NULL,
                  "name" text NOT NULL,
                  "rule_order" integer NOT NULL,
                  "pattern" text NOT NULL,
                  "active" boolean NOT NULL DEFAULT 1
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("matching_rule table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "resource" (
                  "id" text NOT NULL PRIMARY KEY,
                  "title" text NOT NULL,
                  "source_url" text NOT NULL,
                  "source_key" text NOT NULL,
                  "published_at" integer NULL,
                  "created_at" integer NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("resource table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "resource_source" (
                  "resource_id" text NOT NULL,
                  "source_key" text NOT NULL,
                  "source_url" text NOT NULL,
                  "first_seen_at" integer NOT NULL,
                  "last_seen_at" integer NOT NULL,
                  PRIMARY KEY ("resource_id", "source_key", "source_url")
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("resource_source table init failed", error))?;

        query(
            r#"CREATE TABLE IF NOT EXISTS "subscription_space" (
                  "id" integer NOT NULL PRIMARY KEY,
                  "kind" text NOT NULL,
                  "owner_user_id" integer NULL,
                  "team_id" integer NULL,
                  "activation_status" text NOT NULL,
                  "auto_subscribe" boolean NOT NULL DEFAULT 0
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("subscription_space table init failed", error))?;

        query(
            r#"CREATE TABLE IF NOT EXISTS "user_download_driver" (
                  "user_id" integer NOT NULL PRIMARY KEY,
                  "driver_key" text NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("user_download_driver table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "user_download_config" (
                  "user_id" integer NOT NULL PRIMARY KEY,
                  "endpoint" text NOT NULL,
                  "username" text NOT NULL,
                  "secret" text NOT NULL,
                  "download_path" text NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("user_download_config table init failed", error))?;
        query(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "subscription_space_owner_personal_idx"
               ON "subscription_space" ("owner_user_id")
               WHERE "kind" = 'personal';"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            DomainError::external("subscription_space owner index init failed", error)
        })?;

        query(
            r#"CREATE TABLE IF NOT EXISTS "user_space_binding" (
                  "user_id" integer NOT NULL PRIMARY KEY,
                  "personal_space_id" integer NOT NULL,
                  "active_space_id" integer NOT NULL
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("user_space_binding table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "search_pool" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
                  "anime_id" integer NOT NULL,
                  "feed_id" text NOT NULL,
                  "keyword" text NOT NULL,
                  "search_url" text NOT NULL,
                  "created_at" integer NOT NULL,
                  UNIQUE("anime_id", "feed_id", "keyword")
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("search_pool table init failed", error))?;
        query(
            r#"CREATE TABLE IF NOT EXISTS "search_pool_sub" (
                  "pool_id" integer NOT NULL REFERENCES "search_pool"("id") ON DELETE CASCADE,
                  "user_id" integer NOT NULL,
                  "space_id" integer NOT NULL,
                  "anime_id" integer NOT NULL,
                  PRIMARY KEY ("pool_id", "user_id", "space_id", "anime_id")
                 );"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("search_pool_sub table init failed", error))?;
        self.ensure_current_columns(transaction).await?;
        self.migrate_schema(transaction).await?;
        self.ensure_current_table_shapes(transaction).await?;
        Ok(())
    }

    async fn ensure_current_columns(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        if self
            .column_missing(&mut *transaction, "anime_subscription", "user_id")
            .await?
        {
            query(r#"ALTER TABLE "anime_subscription" ADD COLUMN "user_id" integer NULL"#)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("anime_subscription user_id add failed", error)
                })?;
            query(
                r#"UPDATE "anime_subscription"
                   SET user_id = (
                     SELECT owner_user_id
                     FROM "subscription_space" ss
                     WHERE ss.id = "anime_subscription".space_id
                       AND ss.kind = 'personal'
                   )
                   WHERE user_id IS NULL"#,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("anime_subscription user_id migrate failed", error)
            })?;
        }

        if self
            .column_missing(&mut *transaction, "anime_subscription", "search_state")
            .await?
        {
            query(
                r#"ALTER TABLE "anime_subscription" ADD COLUMN "search_state" integer NOT NULL DEFAULT 0"#,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("anime_subscription search_state add failed", error)
            })?;
        }

        if self
            .column_missing(&mut *transaction, "download_record", "user_id")
            .await?
        {
            query(r#"ALTER TABLE "download_record" ADD COLUMN "user_id" integer NULL"#)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("download_record user_id add failed", error)
                })?;
            query(
                r#"UPDATE "download_record"
                   SET user_id = (
                     SELECT owner_user_id
                     FROM "subscription_space" ss
                     WHERE ss.id = "download_record".space_id
                       AND ss.kind = 'personal'
                   )
                   WHERE user_id IS NULL"#,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("download_record user_id migrate failed", error)
            })?;
        }

        for column in [
            (
                "feed_source",
                "source_key",
                r#"ALTER TABLE "feed_source" ADD COLUMN "source_key" text NULL"#,
            ),
            (
                "resource",
                "source_key",
                r#"ALTER TABLE "resource" ADD COLUMN "source_key" text NULL"#,
            ),
            (
                "resource_source",
                "source_key",
                r#"ALTER TABLE "resource_source" ADD COLUMN "source_key" text NULL"#,
            ),
            (
                "matching_rule",
                "active",
                r#"ALTER TABLE "matching_rule" ADD COLUMN "active" boolean NOT NULL DEFAULT 1"#,
            ),
        ] {
            if self
                .column_missing(&mut *transaction, column.0, column.1)
                .await?
            {
                query(column.2)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| DomainError::external("schema column add failed", error))?;
            }
        }

        if self
            .column_exists(&mut *transaction, "resource", "source_name")
            .await?
        {
            query(
                r#"UPDATE "resource"
                   SET source_key = source_name
                   WHERE source_key IS NULL AND source_name IS NOT NULL"#,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("resource source_key migrate failed", error))?;
        }

        if self
            .column_exists(&mut *transaction, "resource_source", "source_name")
            .await?
        {
            query(
                r#"UPDATE "resource_source"
                   SET source_key = source_name
                   WHERE source_key IS NULL AND source_name IS NOT NULL"#,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("resource_source source_key migrate failed", error)
            })?;
        }

        query(r#"UPDATE "feed_source" SET owner_scope = 'space' WHERE owner_scope = $1"#)
            .bind("team")
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("feed source owner scope migrate failed", error)
            })?;
        query(r#"UPDATE "matching_rule" SET owner_scope = 'space' WHERE owner_scope = $1"#)
            .bind("team")
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("matching rule owner scope migrate failed", error)
            })?;

        Ok(())
    }

    async fn ensure_current_table_shapes(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        if self
            .column_exists(&mut *transaction, "anime", "rule_name")
            .await?
        {
            rebuild_anime_table_without_legacy_columns(&mut *transaction).await?;
        }

        Ok(())
    }

    async fn migrate_schema(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        let mut current_version = self.load_schema_version(transaction).await?;
        if current_version > LATEST_SCHEMA_VERSION {
            return Err(DomainError::InvariantViolation(
                "database schema version is newer than this binary supports",
            ));
        }

        while current_version < LATEST_SCHEMA_VERSION {
            self.migrate_single_schema_step(transaction, current_version)
                .await?;
            current_version += 1;
        }

        Ok(())
    }

    async fn migrate_single_schema_step(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        from_version: i64,
    ) -> Result<(), DomainError> {
        let to_version = match from_version {
            0 => {
                self.migrate_v0_to_v1(&mut *transaction).await?;
                1
            }
            1 => {
                self.migrate_v1_to_v2(&mut *transaction).await?;
                2
            }
            _ => {
                return Err(DomainError::InvariantViolation(
                    "database schema migration path is not implemented",
                ))
            }
        };

        query(
            r#"INSERT INTO "config" (key, value) VALUES ($1, $2)
               ON CONFLICT(key) DO UPDATE SET value = excluded.value"#,
        )
        .bind(SCHEMA_VERSION_KEY)
        .bind(to_version.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("schema version upsert failed", error))?;

        Ok(())
    }

    async fn load_schema_version(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<i64, DomainError> {
        let row = query_as::<_, (String,)>(r#"SELECT value FROM "config" WHERE key = $1 LIMIT 1"#)
            .bind(SCHEMA_VERSION_KEY)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("schema version select failed", error))?;

        let Some((value,)) = row else {
            return Ok(0);
        };

        value.parse::<i64>().map_err(|error| {
            DomainError::external("schema version parse failed", anyhow::anyhow!(error))
        })
    }

    async fn migrate_v1_to_v2(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        query(
            r#"DELETE FROM "feed_source" WHERE rowid NOT IN (
                SELECT MIN(rowid) FROM "feed_source" WHERE "source_key" IS NOT NULL GROUP BY "source_key"
            )"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("feed_source duplicate cleanup failed", error))?;
        query(
            r#"DELETE FROM "matching_rule" WHERE rowid NOT IN (
                SELECT MIN(rowid) FROM "matching_rule" GROUP BY "owner_scope", "scope_id", "name"
            )"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("matching_rule duplicate cleanup failed", error))?;
        query(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_feed_source_source_key" ON "feed_source"("source_key")"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("feed_source source_key unique index failed", error))?;
        query(
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_matching_rule_scope_name" ON "matching_rule"("owner_scope", "scope_id", "name")"#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("matching_rule scope+name unique index failed", error))?;
        Ok(())
    }

    async fn migrate_v0_to_v1(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        let admin_user_id = self.find_admin_user_id(transaction).await?;
        self.migrate_existing_users_to_personal_spaces(transaction, admin_user_id)
            .await?;
        let has_legacy_state = self.has_legacy_global_state(transaction).await?;

        let Some(admin_user_id) = admin_user_id else {
            if has_legacy_state {
                return Err(DomainError::InvariantViolation(
                    "legacy global data exists but admin user is missing",
                ));
            }
            return Ok(());
        };

        let admin_personal_space_id = self
            .load_personal_space_id_in_tx(transaction, admin_user_id)
            .await?;
        self.migrate_legacy_admin_rules(transaction, admin_personal_space_id)
            .await?;
        self.migrate_legacy_admin_feed_sources(transaction, admin_personal_space_id)
            .await?;
        self.migrate_legacy_admin_download_configuration(transaction, admin_user_id)
            .await?;
        self.migrate_legacy_resources(transaction).await?;
        migrate_anime_info_json(transaction).await?;
        self.migrate_legacy_anime_records(transaction, admin_user_id, admin_personal_space_id)
            .await?;
        self.migrate_legacy_anime_subscriptions(
            transaction,
            admin_user_id,
            admin_personal_space_id,
        )
        .await?;
        self.validate_v1_migration(transaction).await?;
        self.delete_legacy_config_keys(transaction).await?;
        self.drop_legacy_tables(transaction).await?;
        Ok(())
    }

    async fn find_admin_user_id(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<Option<UserId>, DomainError> {
        let row = query_as::<_, (i64,)>(
            r#"SELECT id FROM "user" WHERE chatacter = 'admin' ORDER BY id ASC LIMIT 1"#,
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("admin user select failed", error))?;
        Ok(row.map(|(id,)| UserId(id)))
    }

    async fn has_legacy_global_state(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<bool, DomainError> {
        if self.table_exists(transaction, "rule").await? {
            let has_rule = query_as::<_, (i64,)>(r#"SELECT 1 FROM "rule" LIMIT 1"#)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("legacy rule existence select failed", error)
                })?
                .is_some();
            if has_rule {
                return Ok(true);
            }
        }

        if self.table_exists(transaction, "rss").await? {
            let has_rss = query_as::<_, (i64,)>(r#"SELECT 1 FROM "rss" LIMIT 1"#)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("legacy rss existence select failed", error)
                })?
                .is_some();
            if has_rss {
                return Ok(true);
            }
        }

        if self.table_exists(transaction, "anime_record").await? {
            let has_anime_record = query_as::<_, (i64,)>(r#"SELECT 1 FROM "anime_record" LIMIT 1"#)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("legacy anime record existence select failed", error)
                })?
                .is_some();
            if has_anime_record {
                return Ok(true);
            }
        }

        Ok(query_as::<_, (String,)>(
            r#"SELECT key FROM "config" WHERE key IN ('qbit_config', 'download_path') LIMIT 1"#,
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy config existence select failed", error))?
        .is_some())
    }

    async fn migrate_legacy_admin_rules(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_personal_space_id: SpaceId,
    ) -> Result<(), DomainError> {
        if !self.table_exists(transaction, "rule").await? {
            return Ok(());
        }

        let rows = query_as::<_, StoredLegacyRuleRow>(
            r#"SELECT name, re, cost FROM "rule" ORDER BY cost ASC, name ASC"#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy rule set select failed", error))?;

        for row in rows {
            let rule = MatchingRule::try_from(row)?;
            insert_matching_rule(transaction, "space", admin_personal_space_id.0, &rule).await?;
        }
        Ok(())
    }

    async fn migrate_legacy_admin_feed_sources(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_personal_space_id: SpaceId,
    ) -> Result<(), DomainError> {
        if !self.table_exists(transaction, "rss").await? {
            return Ok(());
        }

        let rows = query_as::<_, StoredLegacyRssRow>(
            r#"SELECT id, url, title, search_url FROM "rss" ORDER BY id ASC"#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy feed source select failed", error))?;

        for row in rows {
            let source = FeedSource::from(row);
            insert_feed_source(transaction, "space", admin_personal_space_id.0, &source).await?;
        }
        Ok(())
    }

    async fn migrate_legacy_admin_download_configuration(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_user_id: UserId,
    ) -> Result<(), DomainError> {
        let qbit_json = query_as::<_, (String,)>(
            r#"SELECT value FROM "config" WHERE key = 'qbit_config' LIMIT 1"#,
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy qbit config select failed", error))?
        .map(|(value,)| value);
        let download_path = query_as::<_, (String,)>(
            r#"SELECT value FROM "config" WHERE key = 'download_path' LIMIT 1"#,
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy download path select failed", error))?
        .map(|(value,)| value);

        let (Some(qbit_json), Some(download_path)) = (qbit_json, download_path) else {
            return Ok(());
        };
        let qbit_config: LegacyQbitConfig = serde_json::from_str(&qbit_json)
            .map_err(|error| DomainError::external("legacy qbit config parse failed", error))?;

        query(
            r#"INSERT INTO "user_download_config" (user_id, endpoint, username, secret, download_path)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT(user_id) DO UPDATE SET
                   endpoint = excluded.endpoint,
                   username = excluded.username,
                   secret = excluded.secret,
                   download_path = excluded.download_path"#,
        )
        .bind(admin_user_id.0)
        .bind(qbit_config.url)
        .bind(qbit_config.username)
        .bind(self.secret_protector.seal(&qbit_config.password)?)
        .bind(download_path)
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy admin qbit profile migrate failed", error))?;
        query(
            r#"INSERT INTO "user_download_driver" (user_id, driver_key)
               VALUES ($1, 'qbit')
               ON CONFLICT(user_id) DO UPDATE SET
                   driver_key = excluded.driver_key"#,
        )
        .bind(admin_user_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            DomainError::external("legacy admin driver binding migrate failed", error)
        })?;
        Ok(())
    }

    async fn migrate_existing_users_to_personal_spaces(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_user_id: Option<UserId>,
    ) -> Result<(), DomainError> {
        let user_ids = query_as::<_, (i64,)>(r#"SELECT id FROM "user" ORDER BY id ASC"#)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("legacy users select failed", error))?;
        let mut next_space_id =
            query_as::<_, (Option<i64>,)>(r#"SELECT MAX(id) FROM "subscription_space""#)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| {
                    DomainError::external("subscription space max id select failed", error)
                })?
                .0
                .map_or(1, |value| value + 1);

        for (user_id,) in user_ids {
            let binding_exists = query_as::<_, (i64,)>(
                r#"SELECT user_id FROM "user_space_binding" WHERE user_id = $1 LIMIT 1"#,
            )
            .bind(user_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("user space binding existence select failed", error)
            })?
            .is_some();
            if binding_exists {
                continue;
            }

            let space_id = next_space_id;
            next_space_id += 1;
            query(
                r#"INSERT INTO "subscription_space" (id, kind, owner_user_id, team_id, activation_status, auto_subscribe)
                   VALUES ($1, 'personal', $2, NULL, 'active', $3)"#,
            )
            .bind(space_id)
            .bind(user_id)
            .bind(admin_user_id.is_some_and(|admin_user_id| admin_user_id.0 == user_id))
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("personal subscription space insert failed", error))?;
            query(
                r#"INSERT INTO "user_space_binding" (user_id, personal_space_id, active_space_id)
                   VALUES ($1, $2, $2)"#,
            )
            .bind(user_id)
            .bind(space_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("user space binding insert failed", error))?;
        }

        Ok(())
    }

    async fn load_personal_space_id_in_tx(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        user_id: UserId,
    ) -> Result<SpaceId, DomainError> {
        query_as::<_, (i64,)>(
            r#"SELECT personal_space_id FROM "user_space_binding" WHERE user_id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("personal space binding select failed", error))?
        .map(|(space_id,)| SpaceId(space_id))
        .ok_or(DomainError::InvariantViolation(
            "personal space binding missing",
        ))
    }

    async fn migrate_legacy_anime_records(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_user_id: UserId,
        admin_personal_space_id: SpaceId,
    ) -> Result<(), DomainError> {
        if !self.table_exists(transaction, "anime_record").await? {
            return Ok(());
        }

        let rows = query_as::<_, StoredAnimeRecordRow>(
            r#"SELECT title, anime_id, magnet, rule_name, info_hash, created_time FROM "anime_record" ORDER BY created_time ASC"#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy anime record select failed", error))?;

        for row in rows {
            query(
                r#"INSERT INTO "download_record"
                   (user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8)
                   ON CONFLICT(user_id, space_id, anime_id, resource_id) DO NOTHING"#,
            )
            .bind(admin_user_id.0)
            .bind(admin_personal_space_id.0)
            .bind(row.anime_id)
            .bind(&row.info_hash)
            .bind(&row.title)
            .bind(&row.magnet)
            .bind(&row.rule_name)
            .bind(row.created_time)
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("legacy download record migrate failed", error))?;
        }

        Ok(())
    }

    async fn migrate_legacy_anime_subscriptions(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        admin_user_id: UserId,
        admin_personal_space_id: SpaceId,
    ) -> Result<(), DomainError> {
        let rows = query_as::<_, (i64, bool, bool, i64, String)>(
            r#"SELECT id, status, is_search, progress, rule_name FROM "anime""#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy anime subscription select failed", error))?;

        for (anime_id, status, is_search, progress, rule_name) in rows {
            let bound_rule_name = if rule_name.trim().is_empty() {
                None
            } else {
                Some(rule_name)
            };
            let search_state = if is_search { 1_i64 } else { 0_i64 };
            query(
                r#"INSERT INTO "anime_subscription"
                   (user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)
                   ON CONFLICT(user_id, space_id, anime_id) DO UPDATE SET
                     enabled = excluded.enabled,
                     bound_rule_name = COALESCE(excluded.bound_rule_name, "anime_subscription".bound_rule_name),
                     search_state = excluded.search_state,
                     progress = excluded.progress"#,
            )
            .bind(admin_user_id.0)
            .bind(admin_personal_space_id.0)
            .bind(anime_id)
            .bind(status)
            .bind(bound_rule_name)
            .bind(search_state)
            .bind(progress)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("legacy anime subscription insert failed", error)
            })?;
        }

        Ok(())
    }

    async fn migrate_legacy_resources(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        if !self.table_exists(transaction, "rss_record").await? {
            return Ok(());
        }

        let rows = query_as::<_, StoredLegacyResourceRow>(
            r#"SELECT title, magnet, info_hash, created_time, source, url
               FROM "rss_record"
               ORDER BY created_time ASC"#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("legacy rss_record select failed", error))?;

        for row in rows {
            let source_name = row
                .source
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or(DomainError::InvariantViolation(
                    "legacy rss_record source is missing",
                ))?;
            let source_key = build_source_key(&source_name);
            query(
                r#"INSERT INTO "resource"
                   (id, title, source_url, source_key, published_at, created_at)
                   VALUES ($1, $2, $3, $4, NULL, $5)
                   ON CONFLICT(id) DO NOTHING"#,
            )
            .bind(&row.info_hash)
            .bind(&row.title)
            .bind(&row.magnet)
            .bind(&source_key)
            .bind(row.created_time)
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("legacy resource migrate failed", error))?;

            query(
                r#"INSERT INTO "resource_source"
                   (resource_id, source_key, source_url, first_seen_at, last_seen_at)
                   VALUES ($1, $2, $3, $4, $5)
                   ON CONFLICT(resource_id, source_key, source_url)
                   DO UPDATE SET
                     last_seen_at = CASE
                       WHEN excluded.last_seen_at > "resource_source".last_seen_at
                       THEN excluded.last_seen_at
                       ELSE "resource_source".last_seen_at
                     END"#,
            )
            .bind(&row.info_hash)
            .bind(source_key)
            .bind(&row.magnet)
            .bind(row.created_time)
            .bind(row.created_time)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("legacy resource source migrate failed", error)
            })?;
        }

        self.migrate_legacy_download_record_resources(transaction)
            .await?;

        Ok(())
    }

    async fn migrate_legacy_download_record_resources(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        if !self.table_exists(transaction, "anime_record").await? {
            return Ok(());
        }

        let rows = query_as::<_, StoredAnimeRecordRow>(
            r#"SELECT title, anime_id, magnet, rule_name, info_hash, created_time
               FROM "anime_record"
               ORDER BY created_time ASC"#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| {
            DomainError::external("legacy anime record resource select failed", error)
        })?;

        for row in rows {
            query(
                r#"INSERT INTO "resource"
                   (id, title, source_url, source_key, published_at, created_at)
                   VALUES ($1, $2, $3, 'unknown', NULL, $4)
                   ON CONFLICT(id) DO NOTHING"#,
            )
            .bind(&row.info_hash)
            .bind(&row.title)
            .bind(&row.magnet)
            .bind(row.created_time)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("legacy anime record resource migrate failed", error)
            })?;

            query(
                r#"INSERT INTO "resource_source"
                   (resource_id, source_key, source_url, first_seen_at, last_seen_at)
                   VALUES ($1, 'unknown', $2, $3, $3)
                   ON CONFLICT(resource_id, source_key, source_url)
                   DO UPDATE SET
                     last_seen_at = CASE
                       WHEN excluded.last_seen_at > "resource_source".last_seen_at
                       THEN excluded.last_seen_at
                       ELSE "resource_source".last_seen_at
                     END"#,
            )
            .bind(&row.info_hash)
            .bind(&row.magnet)
            .bind(row.created_time)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("legacy anime record resource source migrate failed", error)
            })?;
        }

        Ok(())
    }

    async fn validate_v1_migration(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "user" u
               LEFT JOIN "user_space_binding" b ON b.user_id = u.id
               WHERE b.user_id IS NULL"#,
            "migration produced users without space binding",
        )
        .await?;
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "user_space_binding" b
               LEFT JOIN "subscription_space" s ON s.id = b.personal_space_id
               WHERE s.id IS NULL"#,
            "migration produced bindings without personal space",
        )
        .await?;
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "anime_subscription" s
               LEFT JOIN "user" u ON u.id = s.user_id
               LEFT JOIN "subscription_space" sp ON sp.id = s.space_id
               LEFT JOIN "anime" a ON a.id = s.anime_id
               WHERE u.id IS NULL OR sp.id IS NULL OR a.id IS NULL"#,
            "migration produced orphan anime subscriptions",
        )
        .await?;
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "download_record" d
               LEFT JOIN "user" u ON u.id = d.user_id
               LEFT JOIN "subscription_space" sp ON sp.id = d.space_id
               LEFT JOIN "anime" a ON a.id = d.anime_id
               LEFT JOIN "resource" r ON r.id = d.resource_id
               WHERE u.id IS NULL OR sp.id IS NULL OR a.id IS NULL OR r.id IS NULL"#,
            "migration produced orphan download records",
        )
        .await?;
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "feed_source" f
               LEFT JOIN "subscription_space" sp ON sp.id = f.scope_id
               WHERE f.owner_scope = 'space' AND sp.id IS NULL"#,
            "migration produced orphan feed sources",
        )
        .await?;
        self.ensure_no_rows(
            transaction,
            r#"SELECT COUNT(*)
               FROM "matching_rule" r
               LEFT JOIN "subscription_space" sp ON sp.id = r.scope_id
               WHERE r.owner_scope = 'space' AND sp.id IS NULL"#,
            "migration produced orphan matching rules",
        )
        .await?;
        Ok(())
    }

    async fn ensure_no_rows(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        sql: &str,
        message: &'static str,
    ) -> Result<(), DomainError> {
        let count = query_as::<_, (i64,)>(sql)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("migration validation query failed", error))?
            .0;
        if count != 0 {
            return Err(DomainError::InvariantViolation(message));
        }
        Ok(())
    }

    async fn delete_legacy_config_keys(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        query(r#"DELETE FROM "config" WHERE key IN ('qbit_config', 'download_path')"#)
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("legacy config delete failed", error))?;
        Ok(())
    }

    async fn drop_legacy_tables(
        &self,
        transaction: &mut sqlx::SqliteConnection,
    ) -> Result<(), DomainError> {
        for table in ["rule", "rss", "rss_record", "anime_record"] {
            query(format!(r#"DROP TABLE IF EXISTS "{table}""#).as_str())
                .execute(&mut *transaction)
                .await
                .map_err(|error| DomainError::external("legacy table drop failed", error))?;
        }
        Ok(())
    }

    async fn table_exists(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        table_name: &str,
    ) -> Result<bool, DomainError> {
        Ok(query_as::<_, (String,)>(
            r#"SELECT name FROM "sqlite_master" WHERE type = 'table' AND name = $1 LIMIT 1"#,
        )
        .bind(table_name)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            DomainError::external("sqlite_master table existence select failed", error)
        })?
        .is_some())
    }

    async fn column_exists(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DomainError> {
        Ok(query_as::<_, (String,)>(
            format!(r#"SELECT name FROM pragma_table_info("{table_name}")"#).as_str(),
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("sqlite pragma table_info failed", error))?
        .into_iter()
        .any(|(name,)| name == column_name))
    }

    async fn column_missing(
        &self,
        transaction: &mut sqlx::SqliteConnection,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DomainError> {
        Ok(!self
            .column_exists(transaction, table_name, column_name)
            .await?)
    }

    pub async fn next_user_account_id_value(&self) -> Result<i64, DomainError> {
        let mut counter = self.user_id_counter.lock().await;
        if let Some(current) = *counter {
            *counter = Some(current + 1);
            return Ok(current);
        }

        let next = query_as::<_, (i64,)>(r#"SELECT COALESCE(MAX(id), 9999) + 1 FROM "user""#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| DomainError::external("next user id select failed", error))?
            .0;
        *counter = Some(next + 1);
        Ok(next)
    }

    async fn next_subscription_space_id_value(&self) -> Result<i64, DomainError> {
        let mut counter = self.subscription_space_id_counter.lock().await;
        if let Some(current) = *counter {
            *counter = Some(current + 1);
            return Ok(current);
        }

        let next =
            query_as::<_, (i64,)>(r#"SELECT COALESCE(MAX(id), 0) + 1 FROM "subscription_space""#)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| {
                    DomainError::external("next subscription space id select failed", error)
                })?
                .0;
        *counter = Some(next + 1);
        Ok(next)
    }
}

#[async_trait]
impl SystemInfrastructureInitializer for SqliteDb {
    async fn initialize_infrastructure(&self, biz: &BizContext) -> Result<(), DomainError> {
        self.initialize_schema(biz).await
    }
}

#[async_trait]
impl BizFactory for SqliteDb {
    async fn open_biz(&self) -> Result<BizContext, DomainError> {
        let id = self.next_biz_id.fetch_add(1, Ordering::Relaxed);
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| DomainError::external("biz context acquire failed", error))?;
        query("BEGIN")
            .execute(&mut *connection)
            .await
            .map_err(|error| DomainError::external("biz context begin failed", error))?;
        tracing::trace!("biz #{id} opened");
        Ok(BizContext::new(
            id,
            Arc::new(SqliteBizProvider {
                state: Arc::new(Mutex::new(SqliteBizState {
                    id,
                    connection,
                    committed: false,
                })),
            }),
        ))
    }
}

#[async_trait]
impl IdSequence for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn IdSequence>, DomainError> {
        Ok(Arc::new(SqliteBizIdentifiers::new(
            self.bind_biz_provider(biz)?,
        )))
    }

    async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError> {
        Ok(SpaceId(self.next_subscription_space_id_value().await?))
    }
}

#[async_trait]
impl IdSequence for SqliteBizIdentifiers {
    async fn next_subscription_space_id(&self) -> Result<SpaceId, DomainError> {
        let mut next_id = self.next_space_id.lock().await;
        if let Some(current) = *next_id {
            *next_id = Some(current + 1);
            return Ok(SpaceId(current));
        }

        let mut state = self.biz.state.lock().await;
        let next =
            query_as::<_, (i64,)>(r#"SELECT COALESCE(MAX(id), 0) + 1 FROM "subscription_space""#)
                .fetch_one(&mut *state.connection)
                .await
                .map_err(|error| {
                    DomainError::external("biz next subscription space id select failed", error)
                })?
                .0;
        *next_id = Some(next + 1);
        Ok(SpaceId(next))
    }
}

#[async_trait]
impl UserIdGenerator for SqliteBizUserIds {
    async fn next_user_id(&self) -> Result<UserId, DomainError> {
        let mut next_id = self.next_id.lock().await;
        if let Some(current) = *next_id {
            *next_id = Some(current + 1);
            return Ok(UserId(current));
        }

        let mut state = self.biz.state.lock().await;
        let next = query_as::<_, (i64,)>(r#"SELECT COALESCE(MAX(id), 9999) + 1 FROM "user""#)
            .fetch_one(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz next user id select failed", error))?
            .0;
        *next_id = Some(next + 1);
        Ok(UserId(next))
    }
}

#[async_trait]
impl SubscriptionAnimeRepository for SqliteDb {
    fn with_biz(
        &self,
        biz: &BizContext,
    ) -> Result<Arc<dyn SubscriptionAnimeRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn find_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime subscription select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn pick_one_pending(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE search_state = $1 AND enabled = 1
               LIMIT 1"#,
        )
        .bind(encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::Pending,
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("pick one pending subscription failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn pick_one_localmatch(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE search_state = $1
               LIMIT 1"#,
        )
        .bind(encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::LocalMatch,
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("pick one localmatch subscription failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn pick_one_pending_or_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        let local_match = encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::LocalMatch,
        );
        let pending = encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::Pending,
        );
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE (search_state = $1) OR (search_state = $2 AND enabled = 1)
               ORDER BY CASE search_state WHEN $1 THEN 0 ELSE 1 END
               LIMIT 1"#,
        )
        .bind(local_match)
        .bind(pending)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            DomainError::external("pick one pending or localmatch subscription failed", error)
        })?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_subscriptions_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE anime_id = $1
               ORDER BY user_id ASC, space_id ASC"#,
        )
        .bind(anime_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime subscription list by anime failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE space_id = $1
               ORDER BY user_id ASC, anime_id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime subscription list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_subscription_anime_ids_by_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<AnimeId>, DomainError> {
        let rows = query_as::<_, (i64,)>(
            r#"SELECT anime_id FROM "anime_subscription" WHERE space_id = $1"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            DomainError::external("list subscription anime ids by space failed", error)
        })?;
        Ok(rows.into_iter().map(|(id,)| AnimeId(id)).collect())
    }

    async fn list_enabled_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE space_id = $1 AND enabled = 1
               ORDER BY user_id ASC, anime_id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("enabled anime subscription list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_all_enabled_subscriptions(&self) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE enabled = 1
               ORDER BY space_id ASC, user_id ASC, anime_id ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            DomainError::external("all enabled anime subscription list failed", error)
        })?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn has_enabled_subscription(&self, anime_id: AnimeId) -> Result<bool, DomainError> {
        Ok(query_as::<_, (i64,)>(
            r#"SELECT 1
               FROM "anime_subscription"
               WHERE anime_id = $1 AND enabled = 1
               LIMIT 1"#,
        )
        .bind(anime_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime enabled subscription select failed", error))?
        .is_some())
    }

    async fn save_subscription(&self, subscription: &SubscriptionAnime) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "anime_subscription"
               (user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT(user_id, space_id, anime_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   bound_rule_name = excluded.bound_rule_name,
                   search_state = excluded.search_state,
                   progress = excluded.progress"#,
        )
        .bind(subscription.user_id.0)
        .bind(subscription.space_id.0)
        .bind(subscription.anime_id.0)
        .bind(subscription.enabled)
        .bind(&subscription.bound_rule_name)
        .bind(encode_subscription_search_state(subscription.search_state))
        .bind(subscription.progress)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime subscription upsert failed", error))?;
        Ok(())
    }

    async fn save_subscription_batch(
        &self,
        subscriptions: &[&SubscriptionAnime],
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| DomainError::external("subscription batch acquire failed", e))?;
        let mut tx = conn
            .begin()
            .await
            .map_err(|e| DomainError::external("subscription batch begin failed", e))?;
        for subscription in subscriptions {
            query(
                r#"INSERT INTO "anime_subscription"
                   (user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)
                   ON CONFLICT(user_id, space_id, anime_id) DO UPDATE SET
                       enabled = excluded.enabled,
                       bound_rule_name = excluded.bound_rule_name,
                       search_state = excluded.search_state,
                       progress = excluded.progress"#,
            )
            .bind(subscription.user_id.0)
            .bind(subscription.space_id.0)
            .bind(subscription.anime_id.0)
            .bind(subscription.enabled)
            .bind(&subscription.bound_rule_name)
            .bind(encode_subscription_search_state(subscription.search_state))
            .bind(subscription.progress)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::external("subscription batch upsert failed", e))?;
        }
        tx.commit()
            .await
            .map_err(|e| DomainError::external("subscription batch commit failed", e))?;
        Ok(())
    }

    async fn delete_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::external("begin delete subscription tx failed", e))?;
        query(
            r#"DELETE FROM "anime_subscription" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
            .bind(user_id.0)
            .bind(space_id.0)
            .bind(anime_id.0)
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::external("anime subscription delete failed", error))?;
        query(
            r#"DELETE FROM "download_record" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
            .bind(user_id.0)
            .bind(space_id.0)
            .bind(anime_id.0)
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::external("delete download records failed", error))?;
        query(
            r#"DELETE FROM "search_pool_sub" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
            .bind(user_id.0)
            .bind(space_id.0)
            .bind(anime_id.0)
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::external("delete search_pool_sub records failed", error))?;
        query(
            r#"DELETE FROM "search_pool" WHERE id NOT IN (SELECT DISTINCT pool_id FROM "search_pool_sub")"#,
        )
            .execute(&mut *tx)
            .await
            .map_err(|error| DomainError::external("delete orphan search_pool entries failed", error))?;
        tx.commit()
            .await
            .map_err(|e| DomainError::external("commit delete subscription tx failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionToggleCap for SqliteDb {
    async fn write_enabled(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        enabled: bool,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(r#"UPDATE "anime_subscription" SET enabled = $1 WHERE user_id = $2 AND space_id = $3 AND anime_id = $4"#)
            .bind(enabled)
            .bind(pk.0 .0)
            .bind(pk.1 .0)
            .bind(pk.2 .0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::external("update enabled failed", e))?;
        Ok(())
    }

    async fn with_biz(
        &self,
        biz: &domain::shared::biz::BizContext,
    ) -> Result<Arc<dyn domain::subscription::capability::SubscriptionToggleCap>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionMatchCap for SqliteDb {
    async fn write_match_result(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        progress: i64,
        bound_rule: Option<String>,
        enabled: bool,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"UPDATE "anime_subscription" SET progress = $1, bound_rule_name = $2, enabled = $3
               WHERE user_id = $4 AND space_id = $5 AND anime_id = $6"#,
        )
        .bind(progress)
        .bind(bound_rule)
        .bind(enabled)
        .bind(pk.0 .0)
        .bind(pk.1 .0)
        .bind(pk.2 .0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::external("update match result failed", e))?;
        Ok(())
    }

    async fn with_biz(
        &self,
        biz: &domain::shared::biz::BizContext,
    ) -> Result<Arc<dyn domain::subscription::capability::SubscriptionMatchCap>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionSearchCap for SqliteDb {
    async fn write_search_state(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        state: domain::subscription::SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"UPDATE "anime_subscription" SET search_state = $1 WHERE user_id = $2 AND space_id = $3 AND anime_id = $4"#,
        )
        .bind(encode_subscription_search_state(state))
        .bind(pk.0 .0)
        .bind(pk.1 .0)
        .bind(pk.2 .0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::external("update search_state failed", e))?;
        Ok(())
    }

    async fn batch_write_search_state(
        &self,
        pks: &[domain::subscription::capability::SubscriptionPk],
        state: domain::subscription::SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        if pks.is_empty() {
            return Ok(());
        }
        let _guard = self.write_lock.lock().await;
        let mut qb =
            QueryBuilder::<sqlx::Sqlite>::new(r#"UPDATE "anime_subscription" SET search_state = "#);
        let encoded = encode_subscription_search_state(state);
        qb.push_bind(encoded);
        qb.push(r#" WHERE (user_id, space_id, anime_id) IN ("#);
        for (i, pk) in pks.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push("(");
            qb.push_bind(pk.0 .0);
            qb.push(", ");
            qb.push_bind(pk.1 .0);
            qb.push(", ");
            qb.push_bind(pk.2 .0);
            qb.push(")");
        }
        qb.push(")");
        qb.build()
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::external("batch update search_state failed", e))?;
        Ok(())
    }

    async fn with_biz(
        &self,
        biz: &domain::shared::biz::BizContext,
    ) -> Result<Arc<dyn domain::subscription::capability::SubscriptionSearchCap>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }
}

#[async_trait]
impl SubscriptionAnimeRepository for SqliteBizDb {
    async fn find_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz anime subscription select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE space_id = $1
               ORDER BY user_id ASC, anime_id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz anime subscription list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_subscription_anime_ids_by_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<AnimeId>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, (i64,)>(
            r#"SELECT anime_id FROM "anime_subscription" WHERE space_id = $1"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz list subscription anime ids by space failed", error)
        })?;
        Ok(rows.into_iter().map(|(id,)| AnimeId(id)).collect())
    }

    async fn list_enabled_subscriptions(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE space_id = $1 AND enabled = 1
               ORDER BY user_id ASC, anime_id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz enabled anime subscription list failed", error)
        })?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_all_enabled_subscriptions(&self) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE enabled = 1
               ORDER BY space_id ASC, user_id ASC, anime_id ASC"#,
        )
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz all enabled anime subscription list failed", error)
        })?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn pick_one_pending(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE search_state = $1 AND enabled = 1
               LIMIT 1"#,
        )
        .bind(encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::Pending,
        ))
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz pick one pending subscription failed", error)
        })?;

        row.map(TryInto::try_into).transpose()
    }

    async fn pick_one_localmatch(&self) -> Result<Option<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE search_state = $1
               LIMIT 1"#,
        )
        .bind(encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::LocalMatch,
        ))
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz pick one localmatch subscription failed", error)
        })?;

        row.map(TryInto::try_into).transpose()
    }

    async fn pick_one_pending_or_localmatch(
        &self,
    ) -> Result<Option<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let local_match = encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::LocalMatch,
        );
        let pending = encode_subscription_search_state(
            domain::subscription::SubscriptionSearchState::Pending,
        );
        let row = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE (search_state = $1) OR (search_state = $2 AND enabled = 1)
               ORDER BY CASE search_state WHEN $1 THEN 0 ELSE 1 END
               LIMIT 1"#,
        )
        .bind(local_match)
        .bind(pending)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external(
                "biz pick one pending or localmatch subscription failed",
                error,
            )
        })?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_subscriptions_by_anime(
        &self,
        anime_id: AnimeId,
    ) -> Result<Vec<SubscriptionAnime>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredSubscriptionAnimeRow>(
            r#"SELECT user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress
               FROM "anime_subscription"
               WHERE anime_id = $1
               ORDER BY user_id ASC, space_id ASC"#,
        )
        .bind(anime_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz anime subscription list by anime failed", error)
        })?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn has_enabled_subscription(&self, anime_id: AnimeId) -> Result<bool, DomainError> {
        let mut state = self.biz.state.lock().await;
        Ok(query_as::<_, (i64,)>(
            r#"SELECT 1
               FROM "anime_subscription"
               WHERE anime_id = $1 AND enabled = 1
               LIMIT 1"#,
        )
        .bind(anime_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| {
            DomainError::external("biz anime enabled subscription select failed", error)
        })?
        .is_some())
    }

    async fn save_subscription(&self, subscription: &SubscriptionAnime) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"INSERT INTO "anime_subscription"
               (user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT(user_id, space_id, anime_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   bound_rule_name = excluded.bound_rule_name,
                   search_state = excluded.search_state,
                   progress = excluded.progress"#,
        )
        .bind(subscription.user_id.0)
        .bind(subscription.space_id.0)
        .bind(subscription.anime_id.0)
        .bind(subscription.enabled)
        .bind(&subscription.bound_rule_name)
        .bind(encode_subscription_search_state(subscription.search_state))
        .bind(subscription.progress)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz anime subscription upsert failed", error))?;
        Ok(())
    }

    async fn save_subscription_batch(
        &self,
        subscriptions: &[&SubscriptionAnime],
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        let mut tx = state
            .connection
            .begin()
            .await
            .map_err(|e| DomainError::external("biz subscription batch begin failed", e))?;
        for subscription in subscriptions {
            query(
                r#"INSERT INTO "anime_subscription"
                   (user_id, space_id, anime_id, enabled, bound_rule_name, search_state, progress)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)
                   ON CONFLICT(user_id, space_id, anime_id) DO UPDATE SET
                       enabled = excluded.enabled,
                       bound_rule_name = excluded.bound_rule_name,
                       search_state = excluded.search_state,
                       progress = excluded.progress"#,
            )
            .bind(subscription.user_id.0)
            .bind(subscription.space_id.0)
            .bind(subscription.anime_id.0)
            .bind(subscription.enabled)
            .bind(&subscription.bound_rule_name)
            .bind(encode_subscription_search_state(subscription.search_state))
            .bind(subscription.progress)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::external("biz subscription batch upsert failed", e))?;
        }
        tx.commit()
            .await
            .map_err(|e| DomainError::external("biz subscription batch commit failed", e))?;
        Ok(())
    }

    async fn delete_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"DELETE FROM "anime_subscription" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz anime subscription delete failed", error))?;
        query(
            r#"DELETE FROM "download_record" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz delete download records failed", error))?;
        query(
            r#"DELETE FROM "search_pool_sub" WHERE user_id = $1 AND space_id = $2 AND anime_id = $3"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz delete search_pool_sub records failed", error))?;
        query(
            r#"DELETE FROM "search_pool" WHERE id NOT IN (SELECT DISTINCT pool_id FROM "search_pool_sub")"#,
        )
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz delete orphan search_pool entries failed", error))?;
        Ok(())
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionToggleCap for SqliteBizDb {
    async fn write_enabled(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        enabled: bool,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(r#"UPDATE "anime_subscription" SET enabled = $1 WHERE user_id = $2 AND space_id = $3 AND anime_id = $4"#)
            .bind(enabled)
            .bind(pk.0 .0)
            .bind(pk.1 .0)
            .bind(pk.2 .0)
            .execute(&mut *state.connection)
            .await
            .map_err(|e| DomainError::external("biz update enabled failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionMatchCap for SqliteBizDb {
    async fn write_match_result(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        progress: i64,
        bound_rule: Option<String>,
        enabled: bool,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"UPDATE "anime_subscription" SET progress = $1, bound_rule_name = $2, enabled = $3
               WHERE user_id = $4 AND space_id = $5 AND anime_id = $6"#,
        )
        .bind(progress)
        .bind(bound_rule)
        .bind(enabled)
        .bind(pk.0 .0)
        .bind(pk.1 .0)
        .bind(pk.2 .0)
        .execute(&mut *state.connection)
        .await
        .map_err(|e| DomainError::external("biz update match result failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl domain::subscription::capability::SubscriptionSearchCap for SqliteBizDb {
    async fn write_search_state(
        &self,
        pk: domain::subscription::capability::SubscriptionPk,
        search_state: domain::subscription::SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        let mut biz_state = self.biz.state.lock().await;
        query(
            r#"UPDATE "anime_subscription" SET search_state = $1 WHERE user_id = $2 AND space_id = $3 AND anime_id = $4"#,
        )
        .bind(encode_subscription_search_state(search_state))
        .bind(pk.0 .0)
        .bind(pk.1 .0)
        .bind(pk.2 .0)
        .execute(&mut *biz_state.connection)
        .await
        .map_err(|e| DomainError::external("biz update search_state failed", e))?;
        Ok(())
    }

    async fn batch_write_search_state(
        &self,
        pks: &[domain::subscription::capability::SubscriptionPk],
        search_state: domain::subscription::SubscriptionSearchState,
    ) -> Result<(), DomainError> {
        if pks.is_empty() {
            return Ok(());
        }
        let mut biz_state = self.biz.state.lock().await;
        let mut qb =
            QueryBuilder::<sqlx::Sqlite>::new(r#"UPDATE "anime_subscription" SET search_state = "#);
        let encoded = encode_subscription_search_state(search_state);
        qb.push_bind(encoded);
        qb.push(r#" WHERE (user_id, space_id, anime_id) IN ("#);
        for (i, pk) in pks.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push("(");
            qb.push_bind(pk.0 .0);
            qb.push(", ");
            qb.push_bind(pk.1 .0);
            qb.push(", ");
            qb.push_bind(pk.2 .0);
            qb.push(")");
        }
        qb.push(")");
        qb.build()
            .execute(&mut *biz_state.connection)
            .await
            .map_err(|e| DomainError::external("biz batch update search_state failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl MatchRecordRepository for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn MatchRecordRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn list_space_match_records(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        let rows = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE space_id = $1
               ORDER BY created_at ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("space download record list failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        let rows = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3
               ORDER BY created_at ASC"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("download record list failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_latest_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        limit: usize,
    ) -> Result<Vec<LatestMatchRecord>, DomainError> {
        let rows = query_as::<_, StoredLatestMatchRecordRow>(
            r#"WITH latest AS (
                   SELECT
                     d.anime_id,
                     d.matched_rule_name,
                     d.published_at,
                     d.created_at,
                     ROW_NUMBER() OVER (
                       PARTITION BY d.anime_id
                       ORDER BY d.created_at DESC
                     ) - 1 AS latest_offset
                   FROM "download_record" d
                   WHERE d.user_id = $1 AND d.space_id = $2
                   ORDER BY d.created_at DESC
                   LIMIT $3
               )
               SELECT
                 latest.anime_id,
                 COALESCE(s.progress, 0) AS progress,
                 latest.matched_rule_name,
                 latest.published_at,
                 latest.created_at,
                 latest.latest_offset
               FROM latest
               LEFT JOIN "anime_subscription" s
                 ON s.user_id = $1 AND s.space_id = $2 AND s.anime_id = latest.anime_id
               ORDER BY latest.created_at DESC"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DomainError::InvariantViolation("latest record limit is too large"))?,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("latest download record list failed", error))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn find_match_record(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        resource_id: &MatchResourceId,
    ) -> Result<Option<MatchRecord>, DomainError> {
        let row = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3 AND resource_id = $4
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .bind(&resource_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("download record select failed", error))?;

        Ok(row.map(Into::into))
    }

    async fn save_match_record(&self, record: &MatchRecord) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "download_record"
               (user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT(user_id, space_id, anime_id, resource_id) DO NOTHING"#,
        )
        .bind(record.user_id.0)
        .bind(record.space_id.0)
        .bind(record.anime_id.0)
        .bind(&record.resource_id.0)
        .bind(&record.title)
        .bind(&record.source_url)
        .bind(&record.matched_rule_name)
        .bind(record.published_at)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("download record insert failed", error))?;
        Ok(())
    }
}

#[async_trait]
impl SearchPoolRepository for SqliteDb {
    async fn insert_pool_entries(
        &self,
        entries: &[SearchPoolEntryData],
    ) -> Result<Vec<i64>, DomainError> {
        let mut ids = Vec::with_capacity(entries.len());
        let _guard = self.write_lock.lock().await;
        for entry in entries {
            let id = query_as::<_, (i64,)>(
                r#"INSERT INTO "search_pool" ("anime_id", "feed_id", "keyword", "search_url", "created_at")
                   VALUES ($1, $2, $3, $4, $5)
                   ON CONFLICT("anime_id", "feed_id", "keyword") DO UPDATE SET "created_at" = excluded."created_at"
                   RETURNING "id""#,
            )
            .bind(entry.anime_id.0)
            .bind(&entry.feed_id.0)
            .bind(&entry.keyword)
            .bind(&entry.search_url)
            .bind(entry.created_at)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::external("search_pool insert failed", error))?
            .map(|(id,)| id);
            if let Some(id) = id {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    async fn insert_sub_links(&self, links: &[PoolSubLink]) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        for link in links {
            query(
                r#"INSERT OR IGNORE INTO "search_pool_sub" ("pool_id", "user_id", "space_id", "anime_id")
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(link.pool_id)
            .bind(link.user_id.0)
            .bind(link.space_id.0)
            .bind(link.anime_id.0)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external("search_pool_sub insert failed", error))?;
        }
        Ok(())
    }

    async fn list_distinct_feed_ids(&self) -> Result<Vec<FeedSourceId>, DomainError> {
        let rows = query_as::<_, (String,)>(
            r#"SELECT DISTINCT "feed_id" FROM "search_pool" ORDER BY "feed_id" ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("search_pool feed ids list failed", error))?;
        Ok(rows.into_iter().map(|(id,)| FeedSourceId(id)).collect())
    }

    async fn pick_random(
        &self,
        feed_id: &FeedSourceId,
    ) -> Result<Option<SearchPoolEntry>, DomainError> {
        let row = query_as::<_, (i64, i64, String, String, String)>(
            r#"SELECT "id", "anime_id", "keyword", "search_url", "feed_id"
               FROM "search_pool"
               WHERE "feed_id" = $1
               ORDER BY RANDOM() LIMIT 1"#,
        )
        .bind(&feed_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("search_pool pick random failed", error))?;
        Ok(
            row.map(|(id, anime_id, keyword, search_url, _)| SearchPoolEntry {
                id,
                anime_id: AnimeId(anime_id),
                feed_id: feed_id.clone(),
                keyword,
                search_url,
            }),
        )
    }

    async fn delete_entry(&self, id: i64) -> Result<(), DomainError> {
        query(r#"DELETE FROM "search_pool" WHERE "id" = $1"#)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external("search_pool delete failed", error))?;
        Ok(())
    }

    async fn delete_sub_links_by_pool(&self, pool_id: i64) -> Result<(), DomainError> {
        query(r#"DELETE FROM "search_pool_sub" WHERE "pool_id" = $1"#)
            .bind(pool_id)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external("search_pool_sub delete failed", error))?;
        Ok(())
    }

    async fn cleanup_by_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"DELETE FROM "search_pool_sub"
               WHERE "user_id" = $1 AND "space_id" = $2 AND "anime_id" = $3"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("search_pool_sub cleanup by sub failed", error))?;
        query(
            r#"DELETE FROM "search_pool"
               WHERE "id" NOT IN (SELECT DISTINCT "pool_id" FROM "search_pool_sub")"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("search_pool orphan cleanup failed", error))?;
        Ok(())
    }

    async fn count_by_anime(&self, anime_id: AnimeId) -> Result<i64, DomainError> {
        let (count,) =
            query_as::<_, (i64,)>(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = $1"#)
                .bind(anime_id.0)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| {
                    DomainError::external("search_pool count by anime failed", error)
                })?;
        Ok(count)
    }

    async fn count_distinct_anime(&self) -> Result<i64, DomainError> {
        let (count,) =
            query_as::<_, (i64,)>(r#"SELECT COUNT(DISTINCT "anime_id") FROM "search_pool""#)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| {
                    DomainError::external("search_pool count distinct anime failed", error)
                })?;
        Ok(count)
    }

    async fn count_pending_links(&self) -> Result<i64, DomainError> {
        let (count,) = query_as::<_, (i64,)>(r#"SELECT COUNT(*) FROM "search_pool""#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| DomainError::external("search_pool count total failed", error))?;
        Ok(count)
    }

    fn with_biz(
        &self,
        biz: &domain::shared::biz::BizContext,
    ) -> Result<Arc<dyn SearchPoolRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }
}

#[async_trait]
impl SearchPoolRepository for SqliteBizDb {
    async fn insert_pool_entries(
        &self,
        entries: &[SearchPoolEntryData],
    ) -> Result<Vec<i64>, DomainError> {
        let mut ids = Vec::with_capacity(entries.len());
        let mut state = self.biz.state.lock().await;
        for entry in entries {
            let id = query_as::<_, (i64,)>(
                r#"INSERT INTO "search_pool" ("anime_id", "feed_id", "keyword", "search_url", "created_at")
                   VALUES ($1, $2, $3, $4, $5)
                   ON CONFLICT("anime_id", "feed_id", "keyword") DO UPDATE SET "created_at" = excluded."created_at"
                   RETURNING "id""#,
            )
            .bind(entry.anime_id.0)
            .bind(&entry.feed_id.0)
            .bind(&entry.keyword)
            .bind(&entry.search_url)
            .bind(entry.created_at)
            .fetch_one(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz search_pool insert failed", error))?;
            ids.push(id.0);
        }
        Ok(ids)
    }

    async fn insert_sub_links(&self, links: &[PoolSubLink]) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        for link in links {
            query(
                r#"INSERT OR IGNORE INTO "search_pool_sub" ("pool_id", "user_id", "space_id", "anime_id")
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(link.pool_id)
            .bind(link.user_id.0)
            .bind(link.space_id.0)
            .bind(link.anime_id.0)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz search_pool_sub insert failed", error))?;
        }
        Ok(())
    }

    async fn list_distinct_feed_ids(&self) -> Result<Vec<FeedSourceId>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, (String,)>(r#"SELECT DISTINCT "feed_id" FROM "search_pool""#)
            .fetch_all(&mut *state.connection)
            .await
            .map_err(|error| {
                DomainError::external("biz search_pool distinct feed ids failed", error)
            })?;
        Ok(rows.into_iter().map(|(id,)| FeedSourceId(id)).collect())
    }

    async fn pick_random(
        &self,
        feed_id: &FeedSourceId,
    ) -> Result<Option<SearchPoolEntry>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, (i64, i64, String, String, String)>(
            r#"SELECT "id", "anime_id", "feed_id", "keyword", "search_url"
               FROM "search_pool"
               WHERE "feed_id" = $1
               ORDER BY RANDOM()
               LIMIT 1"#,
        )
        .bind(&feed_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz search_pool pick random failed", error))?;
        Ok(row.map(
            |(id, anime_id, feed, keyword, search_url)| SearchPoolEntry {
                id,
                anime_id: AnimeId(anime_id),
                feed_id: FeedSourceId(feed),
                keyword,
                search_url,
            },
        ))
    }

    async fn delete_entry(&self, id: i64) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(r#"DELETE FROM "search_pool" WHERE "id" = $1"#)
            .bind(id)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz search_pool delete failed", error))?;
        Ok(())
    }

    async fn delete_sub_links_by_pool(&self, pool_id: i64) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(r#"DELETE FROM "search_pool_sub" WHERE "pool_id" = $1"#)
            .bind(pool_id)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| {
                DomainError::external("biz search_pool_sub delete by pool failed", error)
            })?;
        Ok(())
    }

    async fn cleanup_by_subscription(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"DELETE FROM "search_pool_sub" WHERE "user_id" = $1 AND "space_id" = $2 AND "anime_id" = $3"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz search_pool_sub cleanup failed", error))?;
        query(
            r#"DELETE FROM "search_pool" WHERE "id" NOT IN (SELECT DISTINCT "pool_id" FROM "search_pool_sub")"#,
        )
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz search_pool orphans cleanup failed", error))?;
        Ok(())
    }

    async fn count_by_anime(&self, anime_id: AnimeId) -> Result<i64, DomainError> {
        let mut state = self.biz.state.lock().await;
        let (count,) =
            query_as::<_, (i64,)>(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = $1"#)
                .bind(anime_id.0)
                .fetch_one(&mut *state.connection)
                .await
                .map_err(|error| {
                    DomainError::external("biz search_pool count by anime failed", error)
                })?;
        Ok(count)
    }

    async fn count_distinct_anime(&self) -> Result<i64, DomainError> {
        let mut state = self.biz.state.lock().await;
        let (count,) =
            query_as::<_, (i64,)>(r#"SELECT COUNT(DISTINCT "anime_id") FROM "search_pool""#)
                .fetch_one(&mut *state.connection)
                .await
                .map_err(|error| {
                    DomainError::external("biz search_pool count distinct anime failed", error)
                })?;
        Ok(count)
    }

    async fn count_pending_links(&self) -> Result<i64, DomainError> {
        let mut state = self.biz.state.lock().await;
        let (count,) = query_as::<_, (i64,)>(r#"SELECT COUNT(*) FROM "search_pool""#)
            .fetch_one(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz search_pool count total failed", error))?;
        Ok(count)
    }
}

#[async_trait]
impl MatchRecordRepository for SqliteBizDb {
    async fn list_space_match_records(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE space_id = $1
               ORDER BY created_at ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space download record list failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
    ) -> Result<Vec<MatchRecord>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3
               ORDER BY created_at ASC"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz download record list failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_latest_match_records(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        limit: usize,
    ) -> Result<Vec<LatestMatchRecord>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredLatestMatchRecordRow>(
            r#"WITH latest AS (
                   SELECT
                     d.anime_id,
                     d.matched_rule_name,
                     d.published_at,
                     d.created_at,
                     ROW_NUMBER() OVER (
                       PARTITION BY d.anime_id
                       ORDER BY d.created_at DESC
                     ) - 1 AS latest_offset
                   FROM "download_record" d
                   WHERE d.user_id = $1 AND d.space_id = $2
                   ORDER BY d.created_at DESC
                   LIMIT $3
               )
               SELECT
                 latest.anime_id,
                 COALESCE(s.progress, 0) AS progress,
                 latest.matched_rule_name,
                 latest.published_at,
                 latest.created_at,
                 latest.latest_offset
               FROM latest
               LEFT JOIN "anime_subscription" s
                 ON s.user_id = $1 AND s.space_id = $2 AND s.anime_id = latest.anime_id
               ORDER BY latest.created_at DESC"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DomainError::InvariantViolation("latest record limit is too large"))?,
        )
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz latest download record list failed", error))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn find_match_record(
        &self,
        user_id: UserId,
        space_id: SpaceId,
        anime_id: AnimeId,
        resource_id: &MatchResourceId,
    ) -> Result<Option<MatchRecord>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredMatchRecordRow>(
            r#"SELECT user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at
               FROM "download_record"
               WHERE user_id = $1 AND space_id = $2 AND anime_id = $3 AND resource_id = $4
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .bind(space_id.0)
        .bind(anime_id.0)
        .bind(&resource_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz download record select failed", error))?;

        Ok(row.map(Into::into))
    }

    async fn save_match_record(&self, record: &MatchRecord) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"INSERT INTO "download_record"
               (user_id, space_id, anime_id, resource_id, title, source_url, matched_rule_name, published_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT(user_id, space_id, anime_id, resource_id) DO NOTHING"#,
        )
        .bind(record.user_id.0)
        .bind(record.space_id.0)
        .bind(record.anime_id.0)
        .bind(&record.resource_id.0)
        .bind(&record.title)
        .bind(&record.source_url)
        .bind(&record.matched_rule_name)
        .bind(record.published_at)
        .bind(record.created_at)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz download record insert failed", error))?;
        Ok(())
    }
}

#[async_trait]
impl AnimeMetadataRepository for SqliteDb {
    async fn create_anime_metadata(&self, metadata: &AnimeMetadata) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        let inserted =
            query("INSERT INTO anime (id, is_lock, is_search, status, anime_info, progress) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(metadata.id.0)
        .bind(false)
        .bind(0_i64)
        .bind(false)
        .bind(serialize_snapshot(metadata)?)
        .bind(0)
        .execute(&self.pool)
        .await;

        match inserted {
            Ok(_) => Ok(()),
            Err(error) if is_unique_constraint_violation(&error) => {
                Err(DomainError::InvariantViolation("anime already exists"))
            }
            Err(error) => Err(DomainError::external("anime row insert failed", error)),
        }
    }

    async fn replace_anime_metadata(
        &self,
        entries: &[AnimeMetadata],
    ) -> Result<domain::anime::ReplaceAnimeMetadataResult, DomainError> {
        let _guard = self.write_lock.lock().await;
        let mut connection =
            self.pool.acquire().await.map_err(|error| {
                DomainError::external("anime transaction acquire failed", error)
            })?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(|error| DomainError::external("anime transaction begin failed", error))?;
        let mut new_anime_ids = Vec::new();

        for entry in entries {
            let existing =
                query_as::<_, StoredAnimeRow>("SELECT * FROM anime WHERE id = $1 LIMIT 1")
                    .bind(entry.id.0)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(|error| DomainError::external("anime row select failed", error))?;

            if let Some(existing) = existing {
                if existing.is_lock {
                    continue;
                }
                query("UPDATE anime SET anime_info = $1 WHERE id = $2")
                    .bind(serialize_snapshot(entry)?)
                    .bind(entry.id.0)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| DomainError::external("anime row update failed", error))?;
            } else {
                query(
                    "INSERT INTO anime (id, is_lock, is_search, status, anime_info, progress) VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(entry.id.0)
                .bind(false)
                .bind(0_i64)
                .bind(false)
                .bind(serialize_snapshot(entry)?)
                .bind(0)
                .execute(&mut *transaction)
                .await
                .map_err(|error| DomainError::external("anime row insert failed", error))?;
                new_anime_ids.push(entry.id);
            }
        }

        transaction
            .commit()
            .await
            .map_err(|error| DomainError::external("anime transaction commit failed", error))?;
        Ok(domain::anime::ReplaceAnimeMetadataResult { new_anime_ids })
    }
}

#[async_trait]
impl AnimeStateRepository for SqliteDb {
    async fn set_metadata_locked(
        &self,
        anime_id: AnimeId,
        locked: bool,
    ) -> Result<(), DomainError> {
        self.update_anime_bool_flag(
            anime_id,
            locked,
            "is_lock",
            "anime metadata lock update failed",
        )
        .await
    }
}

#[async_trait]
impl domain::anime::capability::AnimeLockCap for SqliteDb {
    async fn write_lock_status(&self, anime_id: AnimeId, locked: bool) -> Result<(), DomainError> {
        self.update_anime_bool_flag(anime_id, locked, "is_lock", "anime lock cap write failed")
            .await
    }
}

#[async_trait]
impl domain::anime::capability::AnimeMetadataUpdateCap for SqliteDb {
    async fn update_metadata(
        &self,
        anime_id: AnimeId,
        metadata: &AnimeMetadata,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        sqlx::query("UPDATE anime SET anime_info = $1 WHERE id = $2")
            .bind(serialize_snapshot(metadata)?)
            .bind(anime_id.0)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external("anime metadata cap update failed", error))?;
        Ok(())
    }
}

#[async_trait]
impl AnimeRepository for SqliteDb {
    async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeSnapshot>, DomainError> {
        let rows = query_as::<_, StoredAnimeRow>(
            r#"SELECT
                 a.id,
                 0 AS status,
                 a.anime_info,
                 0 AS search_state,
                 a.is_lock,
                 0 AS progress
               FROM anime a
               ORDER BY a.id ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime list failed", error))?;

        rows.into_iter()
            .filter_map(|row| match anime_row_matches_query(&row, &query) {
                Ok(true) => Some(Ok(row)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .map(|row| row.map(AnimeSnapshot::from))
            .collect()
    }

    async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeSnapshot>, DomainError> {
        let row = query_as::<_, StoredAnimeRow>(
            r#"SELECT
                 a.id,
                 EXISTS(
                   SELECT 1 FROM "anime_subscription" s
                   WHERE s.anime_id = a.id AND s.enabled = 1
                 ) AS status,
                 a.anime_info,
                 0 AS search_state,
                 a.is_lock,
                 COALESCE((
                   SELECT MAX(s.progress) FROM "anime_subscription" s
                   WHERE s.anime_id = a.id AND s.enabled = 1
                 ), 0) AS progress
               FROM anime a
               WHERE a.id = $1
               LIMIT 1"#,
        )
        .bind(anime_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("anime snapshot select failed", error))?;

        Ok(row.map(AnimeSnapshot::from))
    }

    async fn list_by_ids(&self, anime_ids: &[AnimeId]) -> Result<Vec<AnimeSnapshot>, DomainError> {
        if anime_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut query_builder = QueryBuilder::<Sqlite>::new(
            r#"SELECT
                 a.id,
                 EXISTS(
                   SELECT 1 FROM "anime_subscription" s
                   WHERE s.anime_id = a.id AND s.enabled = 1
                 ) AS status,
                 a.anime_info,
                 0 AS search_state,
                 a.is_lock,
                 COALESCE((
                   SELECT MAX(s.progress) FROM "anime_subscription" s
                   WHERE s.anime_id = a.id AND s.enabled = 1
                 ), 0) AS progress
               FROM anime a
               WHERE a.id IN ("#,
        );
        let mut separated = query_builder.separated(", ");
        for anime_id in anime_ids {
            separated.push_bind(anime_id.0);
        }
        separated.push_unseparated(") ORDER BY a.id ASC");

        let rows = query_builder
            .build_query_as::<StoredAnimeRow>()
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::external("anime batch select failed", error))?;

        Ok(rows.into_iter().map(AnimeSnapshot::from).collect())
    }
}

#[async_trait]
impl UserRepository for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn UserRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn find_user(&self, user_id: UserId) -> Result<Option<User>, DomainError> {
        let row = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" WHERE id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("user select by id failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        let row = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" WHERE username = $1 LIMIT 1"#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("user select by username failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn save_user(&self, user: &User) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| DomainError::external("user transaction acquire failed", error))?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(|error| DomainError::external("user transaction begin failed", error))?;
        let role = encode_user_role(user.role);

        let updated = query(
            r#"UPDATE "user" SET username = $1, password = $2, chatacter = $3 WHERE id = $4"#,
        )
        .bind(&user.username.0)
        .bind(&user.password_hash.0)
        .bind(role)
        .bind(user.id.0)
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("user update failed", error))?
        .rows_affected();

        if updated == 0 {
            query(
                r#"INSERT INTO "user" (id, username, password, chatacter) VALUES ($1, $2, $3, $4)"#,
            )
            .bind(user.id.0)
            .bind(&user.username.0)
            .bind(&user.password_hash.0)
            .bind(role)
            .execute(&mut *transaction)
            .await
            .map_err(|error| DomainError::external("user insert failed", error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| DomainError::external("user transaction commit failed", error))?;
        Ok(())
    }

    async fn list_users(&self) -> Result<Vec<User>, DomainError> {
        let rows = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" ORDER BY id ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("user list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[async_trait]
impl UserRepository for SqliteBizDb {
    async fn find_user(&self, user_id: UserId) -> Result<Option<User>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" WHERE id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz user select by id failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" WHERE username = $1 LIMIT 1"#,
        )
        .bind(username)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz user select by username failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn save_user(&self, user: &User) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        let role = encode_user_role(user.role);
        let updated = query(
            r#"UPDATE "user" SET username = $1, password = $2, chatacter = $3 WHERE id = $4"#,
        )
        .bind(&user.username.0)
        .bind(&user.password_hash.0)
        .bind(role)
        .bind(user.id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz user update failed", error))?
        .rows_affected();

        if updated == 0 {
            query(
                r#"INSERT INTO "user" (id, username, password, chatacter) VALUES ($1, $2, $3, $4)"#,
            )
            .bind(user.id.0)
            .bind(&user.username.0)
            .bind(&user.password_hash.0)
            .bind(role)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz user insert failed", error))?;
        }

        Ok(())
    }

    async fn list_users(&self) -> Result<Vec<User>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredUserRow>(
            r#"SELECT id, username, password, chatacter FROM "user" ORDER BY id ASC"#,
        )
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz user list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[async_trait]
impl SpaceFeedRepository for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn SpaceFeedRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn find_space_feeds(&self, space_id: SpaceId) -> Result<Vec<FeedSource>, DomainError> {
        let rows = query_as::<_, StoredFeedSourceRow>(
            r#"SELECT id, title, site_url, search_url, source_key
               FROM "feed_source"
               WHERE owner_scope = 'space' AND scope_id = $1
               ORDER BY id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("space feed source select failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_space_feeds(&self) -> Result<Vec<FeedSource>, DomainError> {
        let rows = query_as::<_, StoredFeedSourceRow>(
            r#"SELECT id, title, site_url, search_url, source_key
               FROM "feed_source"
               WHERE owner_scope = 'space'
               ORDER BY scope_id ASC, id ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("space feed sources list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn save_space_feed(
        &self,
        space_id: SpaceId,
        source: &FeedSource,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "feed_source"
               (id, owner_scope, scope_id, title, site_url, search_url, source_key)
               VALUES ($1, 'space', $2, $3, $4, $5, $6)
               ON CONFLICT(id) DO UPDATE SET
                 owner_scope = excluded.owner_scope,
                 scope_id = excluded.scope_id,
                 title = excluded.title,
                 site_url = excluded.site_url,
                 search_url = excluded.search_url,
                 source_key = excluded.source_key"#,
        )
        .bind(&source.id.0)
        .bind(space_id.0)
        .bind(&source.title)
        .bind(&source.site_url)
        .bind(&source.search_url)
        .bind(&source.source_key)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("space feed source upsert failed", error))?;
        Ok(())
    }

    async fn delete_space_feed(
        &self,
        space_id: SpaceId,
        source_id: &FeedSourceId,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(r#"DELETE FROM "feed_source" WHERE owner_scope = 'space' AND scope_id = $1 AND id = $2"#)
            .bind(space_id.0)
            .bind(&source_id.0)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external("space feed source single delete failed", error))?;
        Ok(())
    }

    async fn update_space_feed_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(r#"UPDATE "feed_source" SET source_key = $2 WHERE id = $1"#)
            .bind(&source_id.0)
            .bind(source_key)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                DomainError::external("space feed source source_key update failed", error)
            })?;
        Ok(())
    }
}

#[async_trait]
impl SpaceFeedRepository for SqliteBizDb {
    async fn find_space_feeds(&self, space_id: SpaceId) -> Result<Vec<FeedSource>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredFeedSourceRow>(
            r#"SELECT id, title, site_url, search_url, source_key
               FROM "feed_source"
               WHERE owner_scope = 'space' AND scope_id = $1
               ORDER BY id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space feed source select failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn list_space_feeds(&self) -> Result<Vec<FeedSource>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredFeedSourceRow>(
            r#"SELECT id, title, site_url, search_url, source_key
               FROM "feed_source"
               WHERE owner_scope = 'space'
               ORDER BY scope_id ASC, id ASC"#,
        )
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space feed sources list failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn save_space_feed(
        &self,
        space_id: SpaceId,
        source: &FeedSource,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"INSERT INTO "feed_source"
               (id, owner_scope, scope_id, title, site_url, search_url, source_key)
               VALUES ($1, 'space', $2, $3, $4, $5, $6)
               ON CONFLICT(id) DO UPDATE SET
                 owner_scope = excluded.owner_scope,
                 scope_id = excluded.scope_id,
                 title = excluded.title,
                 site_url = excluded.site_url,
                 search_url = excluded.search_url,
                 source_key = excluded.source_key"#,
        )
        .bind(&source.id.0)
        .bind(space_id.0)
        .bind(&source.title)
        .bind(&source.site_url)
        .bind(&source.search_url)
        .bind(&source.source_key)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space feed source upsert failed", error))?;
        Ok(())
    }

    async fn delete_space_feed(
        &self,
        space_id: SpaceId,
        source_id: &FeedSourceId,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(r#"DELETE FROM "feed_source" WHERE owner_scope = 'space' AND scope_id = $1 AND id = $2"#)
            .bind(space_id.0)
            .bind(&source_id.0)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| DomainError::external("biz space feed source single delete failed", error))?;
        Ok(())
    }

    async fn update_space_feed_source_key(
        &self,
        source_id: &FeedSourceId,
        source_key: &str,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(r#"UPDATE "feed_source" SET source_key = $2 WHERE id = $1"#)
            .bind(&source_id.0)
            .bind(source_key)
            .execute(&mut *state.connection)
            .await
            .map_err(|error| {
                DomainError::external("biz space feed source source_key update failed", error)
            })?;
        Ok(())
    }
}

#[async_trait]
impl SpaceRuleRepository for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn SpaceRuleRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn find_active_space_rules(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchingRule>, DomainError> {
        let rows = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND active = 1
               ORDER BY rule_order ASC, id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("space rule set select failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn find_space_rule(
        &self,
        space_id: SpaceId,
        rule_id: &MatchingRuleId,
    ) -> Result<Option<MatchingRule>, DomainError> {
        let row = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND id = $2
               LIMIT 1"#,
        )
        .bind(space_id.0)
        .bind(&rule_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("space rule select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_space_rule_by_name(
        &self,
        space_id: SpaceId,
        name: &str,
    ) -> Result<Option<MatchingRule>, DomainError> {
        let row = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND name = $2
               ORDER BY active DESC, rule_order ASC, id ASC
               LIMIT 1"#,
        )
        .bind(space_id.0)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("space rule by name select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn save_space_rule(
        &self,
        space_id: SpaceId,
        rule: &MatchingRule,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        insert_or_update_matching_rule(&self.pool, "space", space_id.0, rule).await
    }
}

#[async_trait]
impl domain::rule::capability::RuleWriterCap for SqliteDb {
    async fn write_rule(
        &self,
        scope: (&str, i64),
        rule_id: &domain::rule::MatchingRuleId,
        name: &str,
        order: u32,
        pattern: &str,
        active: bool,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"UPDATE "matching_rule" SET name = $1, rule_order = $2, pattern = $3, active = $4
               WHERE id = $5 AND owner_scope = $6 AND scope_id = $7"#,
        )
        .bind(name)
        .bind(i64::from(order))
        .bind(pattern)
        .bind(active)
        .bind(&rule_id.0)
        .bind(scope.0)
        .bind(scope.1)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::external("rule writer cap failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl SpaceRuleRepository for SqliteBizDb {
    async fn find_active_space_rules(
        &self,
        space_id: SpaceId,
    ) -> Result<Vec<MatchingRule>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND active = 1
               ORDER BY rule_order ASC, id ASC"#,
        )
        .bind(space_id.0)
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space rule set select failed", error))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn find_space_rule(
        &self,
        space_id: SpaceId,
        rule_id: &MatchingRuleId,
    ) -> Result<Option<MatchingRule>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND id = $2
               LIMIT 1"#,
        )
        .bind(space_id.0)
        .bind(&rule_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space rule select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_space_rule_by_name(
        &self,
        space_id: SpaceId,
        name: &str,
    ) -> Result<Option<MatchingRule>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredMatchingRuleRow>(
            r#"SELECT id, name, rule_order, pattern, active
               FROM "matching_rule"
               WHERE owner_scope = 'space' AND scope_id = $1 AND name = $2
               ORDER BY active DESC, rule_order ASC, id ASC
               LIMIT 1"#,
        )
        .bind(space_id.0)
        .bind(name)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz space rule by name select failed", error))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn save_space_rule(
        &self,
        space_id: SpaceId,
        rule: &MatchingRule,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        insert_or_update_matching_rule_in_tx(&mut state.connection, "space", space_id.0, rule)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl domain::rule::capability::RuleWriterCap for SqliteBizDb {
    async fn write_rule(
        &self,
        scope: (&str, i64),
        rule_id: &domain::rule::MatchingRuleId,
        name: &str,
        order: u32,
        pattern: &str,
        active: bool,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"UPDATE "matching_rule" SET name = $1, rule_order = $2, pattern = $3, active = $4
               WHERE id = $5 AND owner_scope = $6 AND scope_id = $7"#,
        )
        .bind(name)
        .bind(i64::from(order))
        .bind(pattern)
        .bind(active)
        .bind(&rule_id.0)
        .bind(scope.0)
        .bind(scope.1)
        .execute(&mut *state.connection)
        .await
        .map_err(|e| DomainError::external("biz rule writer cap failed", e))?;
        Ok(())
    }
}

#[async_trait]
impl ResourceRepository for SqliteDb {
    async fn find_resource(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Option<Resource>, DomainError> {
        let row = query_as::<_, StoredResourceRow>(
            r#"SELECT id, title, source_url, source_key, published_at, created_at
               FROM "resource"
               WHERE id = $1
               LIMIT 1"#,
        )
        .bind(&resource_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("resource select failed", error))?;

        Ok(row.map(Into::into))
    }

    async fn list_resource_sources(
        &self,
        resource_id: &ResourceId,
    ) -> Result<Vec<ResourceSource>, DomainError> {
        let rows = query_as::<_, StoredResourceSourceRow>(
            r#"SELECT resource_id, source_key, source_url, first_seen_at, last_seen_at
               FROM "resource_source"
               WHERE resource_id = $1
               ORDER BY source_key ASC, source_url ASC"#,
        )
        .bind(&resource_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("resource source list failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save_resource(&self, resource: &Resource) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "resource"
               (id, title, source_url, source_key, published_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT(id) DO NOTHING"#,
        )
        .bind(&resource.id.0)
        .bind(&resource.title)
        .bind(&resource.source_url)
        .bind(&resource.source_key)
        .bind(resource.published_at)
        .bind(resource.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("resource insert failed", error))?;
        Ok(())
    }

    async fn save_resource_source(&self, source: &ResourceSource) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "resource_source"
               (resource_id, source_key, source_url, first_seen_at, last_seen_at)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT(resource_id, source_key, source_url)
               DO UPDATE SET
                 first_seen_at = CASE
                   WHEN excluded.first_seen_at < "resource_source".first_seen_at
                   THEN excluded.first_seen_at
                   ELSE "resource_source".first_seen_at
                 END,
                 last_seen_at = CASE
                   WHEN excluded.last_seen_at > "resource_source".last_seen_at
                   THEN excluded.last_seen_at
                   ELSE "resource_source".last_seen_at
                 END"#,
        )
        .bind(&source.resource_id.0)
        .bind(&source.source_key)
        .bind(&source.source_url)
        .bind(source.first_seen_at)
        .bind(source.last_seen_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("resource_source insert failed", error))?;
        Ok(())
    }

    async fn latest_resources(&self, since: i64) -> Result<Vec<Resource>, DomainError> {
        let rows = query_as::<_, StoredResourceRow>(
            r#"SELECT id, title, source_url, source_key, published_at, created_at
               FROM "resource"
               WHERE created_at >= $1
               ORDER BY created_at DESC"#,
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("resource latest select failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn search_resources(&self, keywords: &[String]) -> Result<Vec<Resource>, DomainError> {
        if keywords.is_empty() {
            return Ok(vec![]);
        }

        let mut query_string = String::from(
            "SELECT id, title, source_url, source_key, published_at, created_at FROM resource WHERE ",
        );
        let mut conditions = Vec::new();
        let mut bind_count = 1;
        let mut keyword_tokens_flat = Vec::new();

        for keyword in keywords {
            let tokens: Vec<&str> = keyword
                .split(|char: char| !char.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect();
            if tokens.is_empty() {
                continue;
            }

            let mut token_conditions = Vec::new();
            for token in &tokens {
                token_conditions.push(format!("title LIKE '%' || ${bind_count} || '%'"));
                bind_count += 1;
                keyword_tokens_flat.push((*token).to_string());
            }
            conditions.push(format!("({})", token_conditions.join(" AND ")));
        }

        if conditions.is_empty() {
            return Ok(vec![]);
        }

        query_string.push_str(&conditions.join(" OR "));
        query_string.push_str(" ORDER BY created_at DESC");

        let mut query = sqlx::query_as::<_, StoredResourceRow>(&query_string);
        for token in keyword_tokens_flat {
            query = query.bind(token);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::external("resource keyword search failed", error))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[async_trait]
#[async_trait]
#[async_trait]
#[async_trait]
#[async_trait]
#[async_trait]
impl SpaceRepository for SqliteDb {
    fn with_biz(&self, biz: &BizContext) -> Result<Arc<dyn SpaceRepository>, DomainError> {
        Ok(Arc::new(self.bind_biz(biz)?))
    }

    async fn save_subscription_space(&self, space: &Space) -> Result<(), DomainError> {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|error| DomainError::external("acquire space db write failed", error))?;
        query(
            r#"INSERT INTO "subscription_space" (id, kind, owner_user_id, team_id, activation_status, auto_subscribe)
               VALUES ($1, 'personal', NULL, NULL, 'active', $2)
               ON CONFLICT (id) DO UPDATE SET auto_subscribe = excluded.auto_subscribe"#,
        )
        .bind(space.id.0)
        .bind(space.auto_subscribe)
        .execute(&mut *conn)
        .await
        .map_err(|error| DomainError::external("save subscription space failed", error))?;
        Ok(())
    }

    async fn find_subscription_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Option<Space>, DomainError> {
        let row = query_as::<_, StoredSpaceRow>(
            r#"SELECT id, auto_subscribe FROM "subscription_space" WHERE id = $1 LIMIT 1"#,
        )
        .bind(space_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("find subscription space failed", error))?;
        Ok(row.map(|r| Space {
            id: SpaceId(r.id),
            auto_subscribe: r.auto_subscribe,
        }))
    }

    async fn find_personal_space_binding(
        &self,
        user_id: UserId,
    ) -> Result<Option<PersonalSpaceBinding>, DomainError> {
        let row = query_as::<_, (i64,)>(
            r#"SELECT personal_space_id FROM "user_space_binding" WHERE user_id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("find personal space binding failed", error))?;
        Ok(row.map(|(space_id,)| PersonalSpaceBinding {
            personal_space_id: SpaceId(space_id),
        }))
    }

    async fn save_personal_space_binding(
        &self,
        user_id: UserId,
        binding: &PersonalSpaceBinding,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "user_space_binding" (user_id, personal_space_id, active_space_id)
               VALUES ($1, $2, $2)
               ON CONFLICT(user_id) DO UPDATE SET
                 personal_space_id = excluded.personal_space_id,
                 active_space_id = excluded.active_space_id"#,
        )
        .bind(user_id.0)
        .bind(binding.personal_space_id.0)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("save personal space binding failed", error))?;
        query(
            r#"UPDATE "subscription_space"
               SET kind = 'personal',
                   owner_user_id = $1,
                   activation_status = 'active'
               WHERE id = $2"#,
        )
        .bind(user_id.0)
        .bind(binding.personal_space_id.0)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("save personal space owner failed", error))?;
        Ok(())
    }

    async fn list_auto_subscribing_spaces(&self) -> Result<Vec<Space>, DomainError> {
        let rows = query_as::<_, StoredSpaceRow>(
            r#"SELECT id, auto_subscribe FROM "subscription_space" WHERE auto_subscribe = 1"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DomainError::external("list auto subscribing spaces failed", error))?;
        Ok(rows
            .into_iter()
            .map(|r| Space {
                id: SpaceId(r.id),
                auto_subscribe: r.auto_subscribe,
            })
            .collect())
    }

    async fn find_personal_space_user_ids(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<Vec<(SpaceId, UserId)>, DomainError> {
        if space_ids.is_empty() {
            return Ok(Vec::new());
        }
        let params: Vec<String> = (1..=space_ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            r#"SELECT personal_space_id, user_id FROM "user_space_binding" WHERE personal_space_id IN ({})"#,
            params.join(", ")
        );
        let mut q = sqlx::query_as::<_, (i64, i64)>(&sql);
        for id in space_ids {
            q = q.bind(id.0);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DomainError::external("find personal space user ids failed", error))?;
        Ok(rows
            .into_iter()
            .map(|(sid, uid)| (SpaceId(sid), UserId(uid)))
            .collect())
    }
}

#[async_trait]
impl SpaceRepository for SqliteBizDb {
    async fn save_subscription_space(&self, space: &Space) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"INSERT INTO "subscription_space" (id, kind, owner_user_id, team_id, activation_status, auto_subscribe)
               VALUES ($1, 'personal', NULL, NULL, 'active', $2)
               ON CONFLICT (id) DO UPDATE SET auto_subscribe = excluded.auto_subscribe"#,
        )
        .bind(space.id.0)
        .bind(space.auto_subscribe)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz save subscription space failed", error))?;
        Ok(())
    }

    async fn find_subscription_space(
        &self,
        space_id: SpaceId,
    ) -> Result<Option<Space>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, StoredSpaceRow>(
            r#"SELECT id, auto_subscribe FROM "subscription_space" WHERE id = $1 LIMIT 1"#,
        )
        .bind(space_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz find subscription space failed", error))?;
        Ok(row.map(|r| Space {
            id: SpaceId(r.id),
            auto_subscribe: r.auto_subscribe,
        }))
    }

    async fn find_personal_space_binding(
        &self,
        user_id: UserId,
    ) -> Result<Option<PersonalSpaceBinding>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let row = query_as::<_, (i64,)>(
            r#"SELECT personal_space_id FROM "user_space_binding" WHERE user_id = $1 LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz find personal space binding failed", error))?;
        Ok(row.map(|(space_id,)| PersonalSpaceBinding {
            personal_space_id: SpaceId(space_id),
        }))
    }

    async fn save_personal_space_binding(
        &self,
        user_id: UserId,
        binding: &PersonalSpaceBinding,
    ) -> Result<(), DomainError> {
        let mut state = self.biz.state.lock().await;
        query(
            r#"INSERT INTO "user_space_binding" (user_id, personal_space_id, active_space_id)
               VALUES ($1, $2, $2)
               ON CONFLICT(user_id) DO UPDATE SET
                 personal_space_id = excluded.personal_space_id,
                 active_space_id = excluded.active_space_id"#,
        )
        .bind(user_id.0)
        .bind(binding.personal_space_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz save personal space binding failed", error))?;
        query(
            r#"UPDATE "subscription_space"
               SET kind = 'personal',
                   owner_user_id = $1,
                   activation_status = 'active'
               WHERE id = $2"#,
        )
        .bind(user_id.0)
        .bind(binding.personal_space_id.0)
        .execute(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz save personal space owner failed", error))?;
        Ok(())
    }

    async fn list_auto_subscribing_spaces(&self) -> Result<Vec<Space>, DomainError> {
        let mut state = self.biz.state.lock().await;
        let rows = query_as::<_, StoredSpaceRow>(
            r#"SELECT id, auto_subscribe FROM "subscription_space" WHERE auto_subscribe = 1"#,
        )
        .fetch_all(&mut *state.connection)
        .await
        .map_err(|error| DomainError::external("biz list auto subscribing spaces failed", error))?;
        Ok(rows
            .into_iter()
            .map(|r| Space {
                id: SpaceId(r.id),
                auto_subscribe: r.auto_subscribe,
            })
            .collect())
    }

    async fn find_personal_space_user_ids(
        &self,
        space_ids: &[SpaceId],
    ) -> Result<Vec<(SpaceId, UserId)>, DomainError> {
        if space_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut state = self.biz.state.lock().await;
        let params: Vec<String> = (1..=space_ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            r#"SELECT personal_space_id, user_id FROM "user_space_binding" WHERE personal_space_id IN ({})"#,
            params.join(", ")
        );
        let mut q = sqlx::query_as::<_, (i64, i64)>(&sql);
        for id in space_ids {
            q = q.bind(id.0);
        }
        let rows = q.fetch_all(&mut *state.connection).await.map_err(|error| {
            DomainError::external("biz find personal space user ids failed", error)
        })?;
        Ok(rows
            .into_iter()
            .map(|(sid, uid)| (SpaceId(sid), UserId(uid)))
            .collect())
    }
}

impl SqliteDb {
    async fn update_anime_bool_flag(
        &self,
        anime_id: AnimeId,
        value: bool,
        column: &'static str,
        error_context: &'static str,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        let statement = format!("UPDATE anime SET {column} = $1 WHERE id = $2 AND {column} != $1");
        let result = query(statement.as_str())
            .bind(value)
            .bind(anime_id.0)
            .execute(&self.pool)
            .await
            .map_err(|error| DomainError::external(error_context, error))?;

        if result.rows_affected() > 0 {
            return Ok(());
        }

        let exists = query("SELECT 1 FROM anime WHERE id = $1 LIMIT 1")
            .bind(anime_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DomainError::external("anime existence select failed", error))?
            .is_some();

        if exists {
            Ok(())
        } else {
            Err(DomainError::InvariantViolation("anime not found"))
        }
    }

    pub(crate) async fn load_user_qbit_download_profile(
        &self,
        user_id: UserId,
    ) -> Result<Option<StoredUserQbitDownloadProfile>, DomainError> {
        let row = query_as::<_, StoredUserQbitDownloadProfileRow>(
            r#"SELECT user_id, endpoint, username, secret, download_path
               FROM "user_download_config"
               WHERE user_id = $1
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            DomainError::external("user qbit download profile select failed", error)
        })?;

        if let Some(row) = row {
            return Ok(Some(StoredUserQbitDownloadProfile {
                user_id: row.user_id,
                endpoint: row.endpoint,
                username: row.username,
                secret: self.secret_protector.open(&row.secret)?,
                download_path: row.download_path,
            }));
        }

        Ok(None)
    }

    /// 读取用户当前选择的下载器。
    pub async fn load_user_download_driver_key(
        &self,
        user_id: UserId,
    ) -> Result<Option<String>, DomainError> {
        let binding = query_as::<_, StoredUserDownloadDriverRow>(
            r#"SELECT user_id, driver_key
               FROM "user_download_driver"
               WHERE user_id = $1
               LIMIT 1"#,
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DomainError::external("user download driver select failed", error))?;

        if let Some(binding) = binding {
            return Ok(Some(binding.driver_key));
        }

        Ok(None)
    }

    pub(crate) async fn save_user_qbit_download_profile(
        &self,
        profile: &StoredUserQbitDownloadProfile,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "user_download_config" (user_id, endpoint, username, secret, download_path)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT(user_id) DO UPDATE SET
                   endpoint = excluded.endpoint,
                   username = excluded.username,
                   secret = excluded.secret,
                   download_path = excluded.download_path"#,
        )
        .bind(profile.user_id)
        .bind(&profile.endpoint)
        .bind(&profile.username)
        .bind(self.secret_protector.seal(&profile.secret)?)
        .bind(&profile.download_path)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("user qbit download profile upsert failed", error))?;
        Ok(())
    }

    pub async fn save_user_download_driver_key(
        &self,
        user_id: UserId,
        driver_key: &str,
    ) -> Result<(), DomainError> {
        let _guard = self.write_lock.lock().await;
        query(
            r#"INSERT INTO "user_download_driver" (user_id, driver_key)
               VALUES ($1, $2)
               ON CONFLICT(user_id) DO UPDATE SET
                   driver_key = excluded.driver_key"#,
        )
        .bind(user_id.0)
        .bind(driver_key)
        .execute(&self.pool)
        .await
        .map_err(|error| DomainError::external("user download driver upsert failed", error))?;
        Ok(())
    }
}

#[async_trait]
impl UserDownloadDriverBindingStore for SqliteDb {
    async fn find_driver_key(&self, user_id: UserId) -> Result<Option<String>, ApplicationError> {
        self.load_user_download_driver_key(user_id)
            .await
            .map_err(ApplicationError::from)
    }

    async fn save_driver_key(
        &self,
        user_id: UserId,
        driver_key: &str,
    ) -> Result<(), ApplicationError> {
        self.save_user_download_driver_key(user_id, driver_key)
            .await
            .map_err(ApplicationError::from)
    }
}

#[async_trait]
impl UserQbitDownloadProfileStore for SqliteDb {
    async fn find_qbit_profile(
        &self,
        user_id: UserId,
    ) -> Result<Option<UserQbitDownloadProfile>, ApplicationError> {
        self.load_user_qbit_download_profile(user_id)
            .await
            .map(|profile| {
                profile.map(|profile| UserQbitDownloadProfile {
                    endpoint: profile.endpoint,
                    username: profile.username,
                    secret: profile.secret,
                    download_path: profile.download_path,
                })
            })
            .map_err(ApplicationError::from)
    }

    async fn save_qbit_profile(
        &self,
        user_id: UserId,
        profile: &UserQbitDownloadProfile,
    ) -> Result<(), ApplicationError> {
        self.save_user_qbit_download_profile(&StoredUserQbitDownloadProfile {
            user_id: user_id.0,
            endpoint: profile.endpoint.clone(),
            username: profile.username.clone(),
            secret: profile.secret.clone(),
            download_path: profile.download_path.clone(),
        })
        .await
        .map_err(ApplicationError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAnimeRow {
    id: i64,
    status: bool,
    metadata: AnimeMetadata,
    search_state: i64,
    is_lock: bool,
    progress: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredAnimeRecordRow {
    title: String,
    anime_id: i64,
    magnet: String,
    rule_name: String,
    info_hash: String,
    created_time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredLegacyResourceRow {
    title: String,
    magnet: String,
    info_hash: String,
    created_time: i64,
    source: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredSubscriptionAnimeRow {
    user_id: i64,
    space_id: i64,
    anime_id: i64,
    enabled: bool,
    bound_rule_name: Option<String>,
    search_state: i64,
    progress: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredMatchRecordRow {
    user_id: i64,
    space_id: i64,
    anime_id: i64,
    resource_id: String,
    title: String,
    source_url: String,
    matched_rule_name: String,
    published_at: Option<i64>,
    created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredLatestMatchRecordRow {
    anime_id: i64,
    progress: i64,
    matched_rule_name: String,
    published_at: Option<i64>,
    created_at: i64,
    latest_offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredUserRow {
    id: i64,
    username: String,
    password: String,
    chatacter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredSpaceRow {
    id: i64,
    auto_subscribe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredUserDownloadDriverRow {
    user_id: i64,
    driver_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredFeedSourceRow {
    id: String,
    title: String,
    site_url: Option<String>,
    search_url: Option<String>,
    source_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredMatchingRuleRow {
    id: String,
    name: String,
    rule_order: i64,
    pattern: String,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredLegacyRuleRow {
    name: String,
    re: String,
    cost: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredLegacyRssRow {
    id: String,
    url: Option<String>,
    title: String,
    search_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredResourceRow {
    id: String,
    title: String,
    source_url: String,
    source_key: String,
    published_at: Option<i64>,
    created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredResourceSourceRow {
    resource_id: String,
    source_key: String,
    source_url: String,
    first_seen_at: i64,
    last_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct StoredUserQbitDownloadProfileRow {
    user_id: i64,
    endpoint: String,
    username: String,
    secret: String,
    download_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct StoredUserQbitDownloadProfile {
    pub(crate) user_id: i64,
    pub(crate) endpoint: String,
    pub(crate) username: String,
    pub(crate) secret: String,
    pub(crate) download_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LegacyQbitConfig {
    url: String,
    username: String,
    password: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for StoredAnimeRow {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let anime_info: String = row.try_get("anime_info")?;
        let snapshot_payload: StoredAnimeSnapshotPayload = serde_json::from_str(&anime_info)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        let search_state = row
            .try_get("search_state")
            .or_else(|_| row.try_get("is_search"))?;
        Ok(Self {
            id: row.try_get("id")?,
            status: row.try_get("status")?,
            metadata: snapshot_payload.into(),
            search_state,
            is_lock: row.try_get("is_lock")?,
            progress: row.try_get("progress")?,
        })
    }
}

impl TryFrom<StoredUserRow> for User {
    type Error = DomainError;

    fn try_from(value: StoredUserRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: UserId(value.id),
            username: Username(value.username),
            password_hash: PasswordHash(value.password),
            role: decode_user_role(&value.chatacter)?,
        })
    }
}

impl TryFrom<StoredFeedSourceRow> for FeedSource {
    type Error = DomainError;

    fn try_from(value: StoredFeedSourceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: FeedSourceId(value.id),
            title: value.title,
            site_url: value.site_url,
            search_url: value.search_url,
            source_key: value.source_key,
        })
    }
}

impl TryFrom<StoredMatchingRuleRow> for MatchingRule {
    type Error = DomainError;

    fn try_from(value: StoredMatchingRuleRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: MatchingRuleId(value.id),
            name: value.name,
            order: u32::try_from(value.rule_order).map_err(|error| {
                DomainError::external("matching rule order decode failed", error)
            })?,
            pattern: value.pattern,
            active: value.active,
        })
    }
}

impl TryFrom<StoredLegacyRuleRow> for MatchingRule {
    type Error = DomainError;

    fn try_from(value: StoredLegacyRuleRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: MatchingRuleId(format!("legacy:{}", value.name)),
            name: value.name,
            order: u32::try_from(value.cost)
                .map_err(|error| DomainError::external("legacy rule order decode failed", error))?,
            pattern: value.re,
            active: true,
        })
    }
}

impl From<StoredLegacyRssRow> for FeedSource {
    fn from(value: StoredLegacyRssRow) -> Self {
        Self {
            id: FeedSourceId(value.id),
            title: value.title,
            site_url: value.url,
            search_url: value.search_url,
            source_key: None,
        }
    }
}

impl TryFrom<StoredSubscriptionAnimeRow> for SubscriptionAnime {
    type Error = DomainError;

    fn try_from(value: StoredSubscriptionAnimeRow) -> Result<Self, Self::Error> {
        Ok(Self {
            user_id: UserId(value.user_id),
            space_id: SpaceId(value.space_id),
            anime_id: AnimeId(value.anime_id),
            enabled: value.enabled,
            bound_rule_name: value.bound_rule_name,
            search_state: decode_subscription_search_state(value.search_state)?,
            progress: value.progress,
        })
    }
}

impl From<StoredResourceRow> for Resource {
    fn from(value: StoredResourceRow) -> Self {
        Self {
            id: ResourceId(value.id),
            title: value.title,
            source_url: value.source_url,
            source_key: value.source_key,
            published_at: value.published_at,
            created_at: value.created_at,
        }
    }
}

impl From<StoredResourceSourceRow> for ResourceSource {
    fn from(value: StoredResourceSourceRow) -> Self {
        Self {
            resource_id: ResourceId(value.resource_id),
            source_key: value.source_key,
            source_url: value.source_url,
            first_seen_at: value.first_seen_at,
            last_seen_at: value.last_seen_at,
        }
    }
}

impl From<StoredMatchRecordRow> for MatchRecord {
    fn from(value: StoredMatchRecordRow) -> Self {
        Self {
            user_id: UserId(value.user_id),
            space_id: SpaceId(value.space_id),
            anime_id: AnimeId(value.anime_id),
            resource_id: MatchResourceId(value.resource_id),
            title: value.title,
            source_url: value.source_url,
            matched_rule_name: value.matched_rule_name,
            published_at: value.published_at,
            created_at: value.created_at,
        }
    }
}

impl TryFrom<StoredLatestMatchRecordRow> for LatestMatchRecord {
    type Error = DomainError;

    fn try_from(value: StoredLatestMatchRecordRow) -> Result<Self, Self::Error> {
        Ok(Self {
            anime_id: AnimeId(value.anime_id),
            progress: value.progress,
            matched_rule_name: value.matched_rule_name,
            published_at: value.published_at,
            created_at: value.created_at,
            offset: u32::try_from(value.latest_offset).map_err(|error| {
                DomainError::external("latest match record offset decode failed", error)
            })?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredAnimeSnapshotPayload {
    id: i64,
    titles: StoredTitleSet,
    broadcast_weekday: i64,
    planned_episode_count: i64,
    air_date: String,
    season: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredTitleSet {
    original_ja: String,
    localized_zh_cn: String,
    localized_zh_tw: String,
    search_name: String,
    aliases: Vec<String>,
}

impl From<StoredAnimeSnapshotPayload> for AnimeMetadata {
    fn from(value: StoredAnimeSnapshotPayload) -> Self {
        Self {
            id: AnimeId(value.id),
            titles: AnimeTitleSet {
                original_ja: value.titles.original_ja,
                localized_zh_cn: value.titles.localized_zh_cn,
                localized_zh_tw: value.titles.localized_zh_tw,
                search_name: value.titles.search_name,
                aliases: value.titles.aliases,
            },
            broadcast_weekday: BroadcastWeekday(value.broadcast_weekday),
            planned_episode_count: PlannedEpisodeCount(value.planned_episode_count),
            air_date: AirDate(value.air_date),
            season: SeasonNumber(value.season),
        }
    }
}

impl From<&AnimeMetadata> for StoredAnimeSnapshotPayload {
    fn from(value: &AnimeMetadata) -> Self {
        Self {
            id: value.id.0,
            titles: StoredTitleSet {
                original_ja: value.titles.original_ja.clone(),
                localized_zh_cn: value.titles.localized_zh_cn.clone(),
                localized_zh_tw: value.titles.localized_zh_tw.clone(),
                search_name: value.titles.search_name.clone(),
                aliases: value.titles.aliases.clone(),
            },
            broadcast_weekday: value.broadcast_weekday.0,
            planned_episode_count: value.planned_episode_count.0,
            air_date: value.air_date.0.clone(),
            season: value.season.0,
        }
    }
}

fn serialize_snapshot(entry: &AnimeMetadata) -> Result<String, DomainError> {
    serde_json::to_string(&StoredAnimeSnapshotPayload::from(entry))
        .map_err(|error| DomainError::external("anime snapshot serialize failed", error))
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyAnimeSnapshotPayload {
    id: i64,
    name: String,
    weekday: i64,
    eps: i64,
    air_date: String,
    name_tw: String,
    name_cn: String,
    season: i64,
    search_name: String,
    alternative_titles: Option<Vec<String>>,
}

async fn migrate_anime_info_json(
    transaction: &mut sqlx::SqliteConnection,
) -> Result<(), DomainError> {
    let rows = sqlx::query(r#"SELECT id, anime_info FROM "anime""#)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime info json migration select failed", error))?;

    for row in rows {
        let anime_id: i64 = row.get(0);
        let anime_info: String = row.get(1);
        let Ok(legacy) = serde_json::from_str::<LegacyAnimeSnapshotPayload>(&anime_info) else {
            continue;
        };
        let metadata = AnimeMetadata {
            id: AnimeId(legacy.id),
            titles: AnimeTitleSet {
                original_ja: legacy.name,
                localized_zh_cn: legacy.name_cn,
                localized_zh_tw: legacy.name_tw,
                search_name: legacy.search_name,
                aliases: legacy.alternative_titles.unwrap_or_default(),
            },
            broadcast_weekday: BroadcastWeekday(legacy.weekday),
            planned_episode_count: PlannedEpisodeCount(legacy.eps),
            air_date: AirDate(legacy.air_date),
            season: SeasonNumber(legacy.season),
        };
        let new_json = serde_json::to_string(&StoredAnimeSnapshotPayload::from(&metadata))
            .map_err(|error| {
                DomainError::external("anime info json migration serialize failed", error)
            })?;
        sqlx::query(r#"UPDATE "anime" SET anime_info = $1 WHERE id = $2"#)
            .bind(&new_json)
            .bind(anime_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DomainError::external("anime info json migration update failed", error)
            })?;
    }

    Ok(())
}

async fn rebuild_anime_table_without_legacy_columns(
    transaction: &mut sqlx::SqliteConnection,
) -> Result<(), DomainError> {
    query(r#"DROP TABLE IF EXISTS "__yanami_anime_rebuild""#)
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime table rebuild cleanup failed", error))?;
    query(
        r#"CREATE TABLE "__yanami_anime_rebuild" (
              "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
              "status" boolean NOT NULL,
              "is_lock" boolean NOT NULL,
              "is_search" integer NOT NULL,
              "progress" integer NOT NULL,
              "anime_info" json_text NOT NULL
             );"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| DomainError::external("anime table rebuild create failed", error))?;
    query(
        r#"INSERT INTO "__yanami_anime_rebuild" (id, status, is_lock, is_search, progress, anime_info)
           SELECT id, status, is_lock, is_search, progress, anime_info FROM "anime""#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| DomainError::external("anime table rebuild copy failed", error))?;
    query(r#"DROP TABLE "anime""#)
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime table rebuild drop failed", error))?;
    query(r#"ALTER TABLE "__yanami_anime_rebuild" RENAME TO "anime""#)
        .execute(&mut *transaction)
        .await
        .map_err(|error| DomainError::external("anime table rebuild rename failed", error))?;
    Ok(())
}

fn is_unique_constraint_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database_error)
            if database_error.message().to_ascii_lowercase().contains("unique")
    )
}

impl From<StoredAnimeRow> for AnimeSnapshot {
    fn from(row: StoredAnimeRow) -> Self {
        Self {
            metadata: row.metadata,
            metadata_locked: row.is_lock,
        }
    }
}

fn anime_row_matches_query(
    row: &StoredAnimeRow,
    query: &AnimeListQuery,
) -> Result<bool, DomainError> {
    if query
        .metadata_locked
        .is_some_and(|locked| row.is_lock != locked)
    {
        return Ok(false);
    }
    if query
        .keyword
        .as_deref()
        .is_some_and(|keyword| !anime_row_matches_keyword(row, keyword))
    {
        return Ok(false);
    }
    if let (Some(year), Some(month)) = (query.year, query.month) {
        let Ok(air_date) = chrono::NaiveDate::parse_from_str(&row.metadata.air_date.0, "%Y-%m-%d")
        else {
            return Ok(false);
        };
        if normalize_season_filter(air_date.year(), air_date.month())? != (year, month) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn normalize_season_filter(year: i32, month: u32) -> Result<(i32, u32), DomainError> {
    match month {
        12 => Ok((year + 1, 1)),
        1..=2 => Ok((year, 1)),
        3..=5 => Ok((year, 4)),
        6..=8 => Ok((year, 7)),
        9..=11 => Ok((year, 10)),
        _ => Err(DomainError::InvariantViolation("month must be 1..=12")),
    }
}

fn anime_row_matches_keyword(row: &StoredAnimeRow, keyword: &str) -> bool {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return true;
    }

    let titles = &row.metadata.titles;
    [
        titles.original_ja.as_str(),
        titles.localized_zh_cn.as_str(),
        titles.localized_zh_tw.as_str(),
        titles.search_name.as_str(),
    ]
    .into_iter()
    .any(|candidate| candidate.to_lowercase().contains(&keyword))
        || titles
            .aliases
            .iter()
            .any(|alias| alias.to_lowercase().contains(&keyword))
}

fn decode_user_role(value: &str) -> Result<UserRole, DomainError> {
    match value {
        "admin" => Ok(UserRole::Admin),
        "user" => Ok(UserRole::User),
        _ => Err(DomainError::InvariantViolation("invalid user role")),
    }
}

fn encode_user_role(value: UserRole) -> &'static str {
    match value {
        UserRole::Admin => "admin",
        UserRole::User => "user",
    }
}

fn decode_subscription_search_state(value: i64) -> Result<SubscriptionSearchState, DomainError> {
    match value {
        0 => Ok(SubscriptionSearchState::Stopped),
        1 => Ok(SubscriptionSearchState::Pending),
        2 => Ok(SubscriptionSearchState::Running),
        3 => Ok(SubscriptionSearchState::LocalMatch),
        _ => Err(DomainError::InvariantViolation(
            "invalid subscription search state",
        )),
    }
}

fn encode_subscription_search_state(value: SubscriptionSearchState) -> i64 {
    match value {
        SubscriptionSearchState::Stopped => 0,
        SubscriptionSearchState::Pending => 1,
        SubscriptionSearchState::Running => 2,
        SubscriptionSearchState::LocalMatch => 3,
    }
}

async fn insert_feed_source(
    transaction: &mut sqlx::SqliteConnection,
    owner_scope: &str,
    scope_id: i64,
    source: &FeedSource,
) -> Result<(), DomainError> {
    query(
        r#"INSERT INTO "feed_source"
           (id, owner_scope, scope_id, title, site_url, search_url, source_key)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(&source.id.0)
    .bind(owner_scope)
    .bind(scope_id)
    .bind(&source.title)
    .bind(&source.site_url)
    .bind(&source.search_url)
    .bind(&source.source_key)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DomainError::external("feed source insert failed", error))?;
    Ok(())
}

async fn insert_matching_rule(
    transaction: &mut sqlx::SqliteConnection,
    owner_scope: &str,
    scope_id: i64,
    rule: &MatchingRule,
) -> Result<(), DomainError> {
    query(
        r#"INSERT INTO "matching_rule" (id, owner_scope, scope_id, name, rule_order, pattern, active)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(&rule.id.0)
    .bind(owner_scope)
    .bind(scope_id)
    .bind(&rule.name)
    .bind(i64::from(rule.order))
    .bind(&rule.pattern)
    .bind(rule.active)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DomainError::external("matching rule insert failed", error))?;
    Ok(())
}

async fn insert_or_update_matching_rule(
    pool: &Pool<Sqlite>,
    owner_scope: &str,
    scope_id: i64,
    rule: &MatchingRule,
) -> Result<(), DomainError> {
    query(
        r#"INSERT INTO "matching_rule" (id, owner_scope, scope_id, name, rule_order, pattern, active)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT(id) DO UPDATE SET
               owner_scope = excluded.owner_scope,
               scope_id = excluded.scope_id,
               name = excluded.name,
               rule_order = excluded.rule_order,
               pattern = excluded.pattern,
               active = excluded.active"#,
    )
    .bind(&rule.id.0)
    .bind(owner_scope)
    .bind(scope_id)
    .bind(&rule.name)
    .bind(i64::from(rule.order))
    .bind(&rule.pattern)
    .bind(rule.active)
    .execute(pool)
    .await
    .map_err(|error| DomainError::external("matching rule upsert failed", error))?;
    Ok(())
}

async fn insert_or_update_matching_rule_in_tx(
    transaction: &mut sqlx::SqliteConnection,
    owner_scope: &str,
    scope_id: i64,
    rule: &MatchingRule,
) -> Result<(), DomainError> {
    query(
        r#"INSERT INTO "matching_rule" (id, owner_scope, scope_id, name, rule_order, pattern, active)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT(id) DO UPDATE SET
               owner_scope = excluded.owner_scope,
               scope_id = excluded.scope_id,
               name = excluded.name,
               rule_order = excluded.rule_order,
               pattern = excluded.pattern,
               active = excluded.active"#,
    )
    .bind(&rule.id.0)
    .bind(owner_scope)
    .bind(scope_id)
    .bind(&rule.name)
    .bind(i64::from(rule.order))
    .bind(&rule.pattern)
    .bind(rule.active)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DomainError::external("biz matching rule upsert failed", error))?;
    Ok(())
}

fn build_source_key(value: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(value.trim().to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod db_tests {
    use std::sync::Arc;

    use domain::anime::{
        AirDate, AnimeMetadataRepository, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount,
        SeasonNumber,
    };
    use service::system::service::SystemService;
    use space::Spaces;
    use tempfile::NamedTempFile;
    use user::users::Users;

    use crate::user::{LegacySha256PasswordService, SqliteUserIdGenerator};

    use super::*;

    #[tokio::test]
    async fn system_initialization_rolls_back_schema_when_business_initialization_fails() {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = Arc::new(
            SqliteDb::connect(&db_url, "test-key")
                .await
                .expect("connect database"),
        );
        let user_accounts = Arc::new(Users::new(
            database.clone(),
            Arc::new(LegacySha256PasswordService),
            Arc::new(SqliteUserIdGenerator::new(database.clone())),
        ));
        let spaces = Arc::new(Spaces::new(database.clone(), database.clone()));
        let service = SystemService::new(database.clone(), database.clone(), user_accounts, spaces);

        let error = service
            .ensure_initialized("moexco", "bad")
            .await
            .expect_err("initialization must fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: password must be at least 6 characters"
        );
        let remaining_tables = query_as::<_, (i64,)>(
            r#"SELECT COUNT(*)
               FROM sqlite_master
               WHERE type = 'table'
                 AND name NOT LIKE 'sqlite_%'"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("count tables")
        .0;
        assert_eq!(remaining_tables, 0);
    }

    #[tokio::test]
    async fn initialization_removes_legacy_anime_rule_name_column() {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::connect(&db_url, "test-key")
            .await
            .expect("connect database");

        query(
            r#"CREATE TABLE "anime" (
                  "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT,
                  "status" boolean NOT NULL,
                  "is_lock" boolean NOT NULL,
                  "is_search" boolean NOT NULL,
                  "progress" integer NOT NULL,
                  "anime_info" json_text NOT NULL,
                  "rule_name" varchar NOT NULL
                 )"#,
        )
        .execute(&database.pool)
        .await
        .expect("create legacy anime table");
        query(
            r#"INSERT INTO "anime" (id, status, is_lock, is_search, progress, anime_info, rule_name)
               VALUES (7, 0, 0, 0, 0, '{}', '')"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert legacy anime row");

        let biz = database.open_biz().await.expect("open biz");
        database
            .initialize_schema(&biz)
            .await
            .expect("initialize schema");
        biz.commit().await.expect("commit schema");
        drop(biz);

        let rule_name_exists = query_as::<_, (String,)>(
            r#"SELECT name FROM pragma_table_info("anime") WHERE name = 'rule_name' LIMIT 1"#,
        )
        .fetch_optional(&database.pool)
        .await
        .expect("read anime columns")
        .is_some();
        assert!(!rule_name_exists);

        database
            .create_anime_metadata(&AnimeMetadata {
                id: AnimeId(8),
                titles: AnimeTitleSet {
                    original_ja: "葬送のフリーレン".to_string(),
                    localized_zh_cn: "葬送的芙莉莲".to_string(),
                    localized_zh_tw: "葬送的芙蓮".to_string(),
                    search_name: "Frieren".to_string(),
                    aliases: Vec::new(),
                },
                broadcast_weekday: BroadcastWeekday(5),
                planned_episode_count: PlannedEpisodeCount(28),
                air_date: AirDate("2026-04-01".to_string()),
                season: SeasonNumber(2),
            })
            .await
            .expect("insert current anime row");
    }

    #[tokio::test]
    async fn matching_rule_save_deactivates_without_hiding_rule_by_name() {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::new(&db_url, "test-key")
            .await
            .expect("initialize database");
        let active_rule = MatchingRule {
            id: MatchingRuleId("ani".to_string()),
            name: "ANi".to_string(),
            order: 1,
            pattern: r"^\[ANi\].*".to_string(),
            active: true,
        };
        database
            .save_space_rule(SpaceId(1), &active_rule)
            .await
            .expect("save active rule");

        let mut inactive_rule = active_rule.clone();
        inactive_rule.active = false;
        database
            .save_space_rule(SpaceId(1), &inactive_rule)
            .await
            .expect("deactivate rule");

        assert!(database
            .find_active_space_rules(SpaceId(1))
            .await
            .expect("find active rules")
            .is_empty());
        let stored = database
            .find_space_rule_by_name(SpaceId(1), "ANi")
            .await
            .expect("find by name")
            .expect("inactive rule");
        assert_eq!(stored.id.0, "ani");
        assert!(!stored.active);
    }

    #[tokio::test]
    async fn biz_context_drop_auto_rollbacks_without_panic() {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::connect(&db_url, "test-key")
            .await
            .expect("connect database");

        // Drop biz without commit — should auto-rollback, not panic
        {
            let _biz = database.open_biz().await.expect("first biz");
        }

        // Second biz after auto-rollback
        {
            let biz = database
                .open_biz()
                .await
                .expect("second biz after auto-rollback");
            biz.commit().await.expect("commit after auto-rollback");
        }

        // With explicit rollback
        {
            let biz = database.open_biz().await.expect("third biz");
            biz.rollback().await.expect("explicit rollback");
        }

        // Fourth biz after explicit rollback
        {
            let biz = database
                .open_biz()
                .await
                .expect("fourth biz after explicit rollback");
            biz.commit().await.expect("commit after explicit rollback");
        }
    }

    use domain::subscription::capability::{SubscriptionSearchCap, SubscriptionToggleCap};

    #[tokio::test]
    async fn biz_disable_cleans_pool_and_writes_enabled_and_stops_search_in_one_tx(
    ) -> Result<(), DomainError> {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::new(&db_url, "test-key")
            .await
            .expect("initialize database");

        database
            .create_anime_metadata(&AnimeMetadata {
                id: AnimeId(1),
                titles: AnimeTitleSet {
                    original_ja: "Test".to_string(),
                    localized_zh_cn: "测试".to_string(),
                    localized_zh_tw: "測試".to_string(),
                    search_name: "test".to_string(),
                    aliases: vec![],
                },
                broadcast_weekday: BroadcastWeekday(1),
                planned_episode_count: PlannedEpisodeCount(12),
                air_date: AirDate("2026-04-01".to_string()),
                season: SeasonNumber(1),
            })
            .await
            .expect("create anime");

        let sub = SubscriptionAnime {
            user_id: UserId(1),
            space_id: SpaceId(1),
            anime_id: AnimeId(1),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Pending,
            progress: 0,
        };
        database
            .save_subscription(&sub)
            .await
            .expect("save subscription");

        // insert pool entries + sub_links directly via SQL
        query(
            r#"INSERT INTO "search_pool" ("anime_id", "feed_id", "keyword", "search_url", "created_at")
               VALUES (1, 'test-feed', 'keyword', 'http://example.com', 1000)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert pool entry");
        query(
            r#"INSERT INTO "search_pool_sub" ("pool_id", "user_id", "space_id", "anime_id")
               VALUES (1, 1, 1, 1)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert sub link");

        // verify initial state
        let pool_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = 1"#)
                .fetch_one(&database.pool)
                .await
                .expect("count pool by anime");
        assert_eq!(pool_count.0, 1, "should have 1 pool entry before disable");

        // biz: cleanup + disable + stop_search
        {
            let biz = database.open_biz().await.expect("open biz");
            let biz_db = database.bind_biz(&biz).expect("bind biz");
            biz_db
                .cleanup_by_subscription(UserId(1), SpaceId(1), AnimeId(1))
                .await
                .expect("cleanup pool");
            biz_db
                .write_enabled((UserId(1), SpaceId(1), AnimeId(1)), false)
                .await
                .expect("write enabled");
            biz_db
                .write_search_state(
                    (UserId(1), SpaceId(1), AnimeId(1)),
                    SubscriptionSearchState::Stopped,
                )
                .await
                .expect("write search state");
            biz.commit().await.expect("commit biz");
        }

        // verify pool entries cleaned
        let pool_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = 1"#)
                .fetch_one(&database.pool)
                .await
                .expect("count pool by anime");
        assert_eq!(pool_count.0, 0, "pool entries should be cleaned");

        // verify sub_links cleaned
        let link_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool_sub" WHERE "anime_id" = 1"#)
                .fetch_one(&database.pool)
                .await
                .expect("count sub links");
        assert_eq!(link_count.0, 0, "sub links should be cleaned");

        // verify subscription state
        let row = query_as::<_, (bool, i64)>(
            r#"SELECT "enabled", "search_state" FROM "anime_subscription"
               WHERE "anime_id" = 1 LIMIT 1"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("fetch subscription");
        assert!(!row.0, "enabled should be false");
        assert_eq!(row.1, 0, "search_state should be Stopped(0)");
        Ok(())
    }

    #[tokio::test]
    async fn delete_subscription_removes_all_related_records() -> Result<(), DomainError> {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::new(&db_url, "test-key")
            .await
            .expect("initialize database");

        database
            .create_anime_metadata(&AnimeMetadata {
                id: AnimeId(2),
                titles: AnimeTitleSet {
                    original_ja: "Test2".to_string(),
                    localized_zh_cn: "测试2".to_string(),
                    localized_zh_tw: "測試2".to_string(),
                    search_name: "test2".to_string(),
                    aliases: vec![],
                },
                broadcast_weekday: BroadcastWeekday(2),
                planned_episode_count: PlannedEpisodeCount(24),
                air_date: AirDate("2026-07-01".to_string()),
                season: SeasonNumber(1),
            })
            .await
            .expect("create anime");

        let sub = SubscriptionAnime {
            user_id: UserId(2),
            space_id: SpaceId(2),
            anime_id: AnimeId(2),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Running,
            progress: 5,
        };
        database
            .save_subscription(&sub)
            .await
            .expect("save subscription");

        query(
            r#"INSERT INTO "download_record"
               ("user_id", "space_id", "anime_id", "resource_id", "title", "source_url",
                "matched_rule_name", "published_at", "created_at")
               VALUES (2, 2, 2, 'res-1', 'ep 5', 'url', 'rule1', 1000, 1000)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert download record");

        query(
            r#"INSERT INTO "search_pool" ("anime_id", "feed_id", "keyword", "search_url", "created_at")
               VALUES (2, 'feed-2', 'kw', 'http://example.com/2', 1000)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert pool entry");
        query(
            r#"INSERT INTO "search_pool_sub" ("pool_id", "user_id", "space_id", "anime_id")
               VALUES (1, 2, 2, 2)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert sub link");

        database
            .delete_subscription(UserId(2), SpaceId(2), AnimeId(2))
            .await
            .expect("delete subscription");

        let sub_exists: (i64,) = query_as(
            r#"SELECT COUNT(*) FROM "anime_subscription"
               WHERE "user_id" = 2 AND "space_id" = 2 AND "anime_id" = 2"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("count subscription");
        assert_eq!(sub_exists.0, 0, "subscription should be deleted");

        let dr_count: (i64,) = query_as(
            r#"SELECT COUNT(*) FROM "download_record"
               WHERE "user_id" = 2 AND "space_id" = 2 AND "anime_id" = 2"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("count download records");
        assert_eq!(dr_count.0, 0, "download records should be deleted");

        let sub_link_count: (i64,) = query_as(
            r#"SELECT COUNT(*) FROM "search_pool_sub"
               WHERE "user_id" = 2 AND "space_id" = 2 AND "anime_id" = 2"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("count sub links");
        assert_eq!(sub_link_count.0, 0, "sub links should be deleted");

        // pool entry should also be deleted (orphaned after sub_link removal)
        let pool_count: (i64,) = query_as(
            r#"SELECT COUNT(*) FROM "search_pool"
               WHERE "anime_id" = 2"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("count pool entries");
        assert_eq!(pool_count.0, 0, "orphan pool entries should be deleted");
        Ok(())
    }

    #[tokio::test]
    async fn biz_stop_search_cleans_pool_and_stops_search_in_one_tx() -> Result<(), DomainError> {
        let db_file = NamedTempFile::new().expect("temp db");
        let db_url = format!("sqlite://{}", db_file.path().display());
        let database = SqliteDb::new(&db_url, "test-key")
            .await
            .expect("initialize database");

        database
            .create_anime_metadata(&AnimeMetadata {
                id: AnimeId(3),
                titles: AnimeTitleSet {
                    original_ja: "Test3".to_string(),
                    localized_zh_cn: "测试3".to_string(),
                    localized_zh_tw: "測試3".to_string(),
                    search_name: "test3".to_string(),
                    aliases: vec![],
                },
                broadcast_weekday: BroadcastWeekday(3),
                planned_episode_count: PlannedEpisodeCount(12),
                air_date: AirDate("2026-10-01".to_string()),
                season: SeasonNumber(1),
            })
            .await
            .expect("create anime");

        let sub = SubscriptionAnime {
            user_id: UserId(3),
            space_id: SpaceId(3),
            anime_id: AnimeId(3),
            enabled: true,
            bound_rule_name: None,
            search_state: SubscriptionSearchState::Running,
            progress: 3,
        };
        database
            .save_subscription(&sub)
            .await
            .expect("save subscription");

        query(
            r#"INSERT INTO "search_pool" ("anime_id", "feed_id", "keyword", "search_url", "created_at")
               VALUES (3, 'feed-3', 'k', 'http://example.com/3', 1000)"#,
        )
        .execute(&database.pool)
        .await
        .expect("insert pool entry");
        let pool_id =
            query_as::<_, (i64,)>(r#"SELECT "id" FROM "search_pool" WHERE "anime_id" = 3 LIMIT 1"#)
                .fetch_one(&database.pool)
                .await
                .expect("get pool id")
                .0;

        query(
            r#"INSERT INTO "search_pool_sub" ("pool_id", "user_id", "space_id", "anime_id")
               VALUES ($1, 3, 3, 3)"#,
        )
        .bind(pool_id)
        .execute(&database.pool)
        .await
        .expect("insert sub link");

        // verify initial pool exists
        let pool_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = 3"#)
                .fetch_one(&database.pool)
                .await
                .expect("count pool before");
        assert_eq!(pool_count.0, 1, "should have 1 pool entry before stop");

        // biz: cleanup + stop_search (equivalent of clean_anime_search_pool)
        {
            let biz = database.open_biz().await.expect("open biz");
            let biz_db = database.bind_biz(&biz).expect("bind biz");
            biz_db
                .cleanup_by_subscription(UserId(3), SpaceId(3), AnimeId(3))
                .await
                .expect("cleanup pool");
            biz_db
                .write_search_state(
                    (UserId(3), SpaceId(3), AnimeId(3)),
                    SubscriptionSearchState::Stopped,
                )
                .await
                .expect("write search state");
            biz.commit().await.expect("commit biz");
        }

        // verify pool cleaned
        let pool_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool" WHERE "anime_id" = 3"#)
                .fetch_one(&database.pool)
                .await
                .expect("count pool after");
        assert_eq!(pool_count.0, 0, "pool entries should be cleaned");

        // verify sub_links cleaned
        let link_count: (i64,) =
            query_as(r#"SELECT COUNT(*) FROM "search_pool_sub" WHERE "anime_id" = 3"#)
                .fetch_one(&database.pool)
                .await
                .expect("count sub links");
        assert_eq!(link_count.0, 0, "sub links should be cleaned");

        // verify enabled unchanged, search_state Stopped
        let row = query_as::<_, (bool, i64)>(
            r#"SELECT "enabled", "search_state" FROM "anime_subscription"
               WHERE "anime_id" = 3 LIMIT 1"#,
        )
        .fetch_one(&database.pool)
        .await
        .expect("fetch subscription");
        assert!(row.0, "enabled should remain true");
        assert_eq!(row.1, 0, "search_state should be Stopped(0)");
        Ok(())
    }
}
