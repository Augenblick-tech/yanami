use anyhow::Result;
use async_trait::async_trait;

use crate::{
    entity::{
        cap::RuleRepository,
        model::{Rule, RuleBaseData, RuleQuery},
    },
    infra::repository::client::RuleSqliteClient,
};

#[async_trait]
impl RuleRepository for RuleSqliteClient {
    async fn list(&self, query: &RuleQuery) -> Result<Vec<RuleBaseData>> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, "order", space_id, pattern, deleted_at FROM rule WHERE 1=1"#,
        );

        // 按空间 ID 过滤
        if let Some(space_id) = query.space_id {
            builder.push(" AND space_id = ");
            builder.push_bind(space_id);
        }

        // 按激活状态过滤
        if let Some(active) = query.active {
            if active {
                builder.push(" AND deleted_at IS NULL");
            } else {
                builder.push(" AND deleted_at IS NOT NULL");
            }
        }

        // order 值越小优先级越高；添加 id ASC 保证稳定排序
        builder.push(r#" ORDER BY "order" ASC, id ASC"#);

        let rows = builder.build().fetch_all(&self.pool).await?;

        // 使用辅助函数将数据逐个映射
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(Self::parse_raw(&row)?);
        }

        Ok(result)
    }

    async fn find(&self, id: i64) -> Result<Option<RuleBaseData>> {
        let row = sqlx::query(
            r#"SELECT id, name, "order", space_id, pattern, deleted_at FROM rule WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref()
            .map(Self::parse_raw)
            .transpose()
            .map_err(Into::into)
    }

    async fn insert(&self, rule: &Rule) -> Result<RuleBaseData> {
        let row = sqlx::query(
            r#"
            INSERT INTO rule (name, "order", space_id, pattern, deleted_at)
            VALUES (?, ?, ?, ?, NULL)
            RETURNING id, name, "order", space_id, pattern, deleted_at
            "#,
        )
        .bind(&rule.name)
        .bind(rule.order)
        .bind(rule.space_id)
        .bind(&rule.pattern)
        .fetch_one(&self.pool)
        .await?;

        Ok(Self::parse_raw(&row)?)
    }

    async fn save(&self, rule: &RuleBaseData) -> Result<()> {
        // 修改规则一律清除对应的缓存
        self.regex_cache.remove(&rule.metadata.pattern);
        let active_flag = if rule.active { 1 } else { 0 };

        sqlx::query(
            r#"
            UPDATE rule
            SET name = ?,
                "order" = ?,
                space_id = ?,
                pattern = ?,
                deleted_at = CASE
                    WHEN ? = 1 THEN NULL
                    ELSE COALESCE(deleted_at, unixepoch())
                END
            WHERE id = ?
            "#,
        )
        .bind(&rule.metadata.name)
        .bind(rule.metadata.order)
        .bind(rule.metadata.space_id)
        .bind(&rule.metadata.pattern)
        .bind(active_flag)
        .bind(rule.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // 如果规则被任意订阅番剧引用，则只能软删除，将deleted_at设置删除时间
    async fn delete(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // 获取锁的同时拿到 pattern；如果 rule 本身都不存在，直接返回（保证接口幂等性）
        let row = sqlx::query(
            r#"SELECT id, name, "order", space_id, pattern, deleted_at FROM rule WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let rule = match row {
            Some(r) => Self::parse_raw(&r)?,
            None => return Ok(()),
        };

        let res = sqlx::query(
            "DELETE FROM rule WHERE id = ? AND NOT EXISTS (SELECT 1 FROM sub_anime WHERE rule_id = ?)"
        )
        .bind(id)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // 如果 rows_affected() == 0，说明被 NOT EXISTS 拦截了（存在被 sub_anime 引用的情况）
        if res.rows_affected() == 0 {
            sqlx::query(
                "UPDATE rule SET deleted_at = COALESCE(deleted_at, unixepoch()) WHERE id = ?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.regex_cache.remove(&rule.metadata.pattern);
        Ok(())
    }
}
