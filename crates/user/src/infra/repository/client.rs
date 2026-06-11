use anyhow::Result;
use async_trait::async_trait;
use common::{infra::app_ctx::AppContext, shared::boss::FromContext};
use sqlx::{Pool, Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::entity::{
    cap::UserRepository,
    model::{UserBaseData, UserProps, UserRole},
};

#[derive(Clone)]
pub struct UserSqliteClient {
    pub(super) pool: Pool<Sqlite>,
}

impl FromContext for UserSqliteClient {
    type Ctx = AppContext;

    fn build_from(ctx: &Self::Ctx) -> Result<Self> {
        Ok(Self {
            pool: ctx.pool.clone(),
        })
    }
}

impl UserSqliteClient {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn init(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.init_with_tx(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn init_with_tx(&self, tx: &mut Transaction<'_, Sqlite>) -> Result<()> {
        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS user (
                id              INTEGER PRIMARY KEY AUTOINCREMENT, 
                username        TEXT NOT NULL UNIQUE,
                password        TEXT NOT NULL,
                role            INTEGER NOT NULL,
                space_id        INTEGER NOT NULL,
                auto_sub        INTEGER NOT NULL DEFAULT 0,
                download_config TEXT
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS space (
                id              INTEGER PRIMARY KEY AUTOINCREMENT
            );",
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_username ON user(username);")
            .execute(&mut **tx)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_auto_sub ON user(auto_sub);")
            .execute(&mut **tx)
            .await?;

        Ok(())
    }
}

impl UserSqliteClient {
    fn parse_row(&self, row: &SqliteRow) -> Result<UserProps> {
        let id: i64 = row.try_get("id")?;
        let username: String = row.try_get("username")?;
        let password: String = row.try_get("password")?;
        let role_u8: u8 = row.try_get("role")?;
        let space_id: i64 = row.try_get("space_id")?;
        let role = UserRole::try_from(role_u8)?;
        let auto_sub: i32 = row.try_get("auto_sub").unwrap_or_default();
        let config_str: Option<String> = row.try_get("download_config").unwrap_or(None);
        let download_config = match config_str {
            Some(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
            _ => Vec::new(),
        };
        let data = UserBaseData {
            id,
            username,
            password,
            role,
            space_id,
            auto_sub: auto_sub == 1,
            download_config,
        };
        Ok(UserProps { data })
    }
}

#[async_trait]
impl UserRepository for UserSqliteClient {
    async fn find_by_username(&self, username: &str) -> Result<Option<UserProps>> {
        let row = sqlx::query(
            "SELECT id, username, password, role, space_id, auto_sub, download_config FROM user WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(self.parse_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_space_id(&self, space_id: i64) -> Result<Option<UserProps>> {
        let row = sqlx::query(
            "SELECT id, username, password, role, space_id, auto_sub, download_config FROM user WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(self.parse_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn find(&self, id: i64) -> Result<Option<UserProps>> {
        let row = sqlx::query(
            "SELECT id, username, password, role, space_id, auto_sub, download_config FROM user WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(self.parse_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn insert(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
        auto_sub: bool,
    ) -> Result<UserProps> {
        let mut tx = self.pool.begin().await?;
        let role_u8: u8 = role.into();
        let space_res = sqlx::query("INSERT INTO space DEFAULT VALUES")
            .execute(&mut *tx)
            .await?;
        let space_id = space_res.last_insert_rowid();
        let result = sqlx::query(
            "INSERT INTO user (username, password, role, space_id, auto_sub, download_config) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(username)
        .bind(password)
        .bind(role_u8)
        .bind(space_id)
        .bind(if auto_sub { 1 } else  { 0 })
        .bind("[]")
        .execute(&mut *tx)
        .await?;

        let id = result.last_insert_rowid();
        let data = UserBaseData {
            id,
            username: username.to_string(),
            password: password.to_string(),
            role,
            space_id,
            auto_sub,
            download_config: Vec::new(),
        };
        tx.commit().await?;
        Ok(UserProps { data })
    }

    async fn update(&self, user: &UserBaseData) -> Result<()> {
        let role_u8: u8 = user.role.into();
        let result = sqlx::query(
            "UPDATE user SET username = ?, password = ?, role = ?, space_id = ?, auto_sub = ?, download_config = ? WHERE id = ?",
        )
        .bind(&user.username)
        .bind(&user.password)
        .bind(role_u8)
        .bind(user.space_id)
        .bind(if user.auto_sub { 1 } else { 0 })
        .bind(serde_json::to_string(&user.download_config)?)
        .bind(user.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("not found user {}", user.id);
        }
        Ok(())
    }

    async fn list_auto_sub(&self) -> Result<Vec<UserProps>> {
        let rows = sqlx::query(
            "SELECT id, username, password, role, space_id, auto_sub, download_config FROM user WHERE auto_sub = 1",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut props = vec![];
        for row in &rows {
            props.push(self.parse_row(row)?);
        }

        Ok(props)
    }

    async fn count_by_role(&self, role: UserRole) -> Result<i64> {
        let role_u8: u8 = role.into();
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user WHERE role = ?")
            .bind(role_u8)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }
}
