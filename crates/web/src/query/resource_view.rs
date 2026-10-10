use anyhow::Result;
use common::shared::str::nfkc_to_lowercase;
use sqlx::{Pool, Row, Sqlite};

use crate::model::{PageResourceRequest, ResourceItem, ResourcePage};

#[derive(Clone)]
pub struct ResourceViewQuery {
    pub pool: Pool<Sqlite>,
}

impl ResourceViewQuery {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// 手动挑资源用的列表：只读库里的列，不经过资源领域的查询。
    ///
    /// 关键字按用户输入原样做 LIKE，前导通配符用不上索引，靠发布时间的范围与每页条数限制扫描量；
    /// 关联订阅时左连接剧集表，标出该资源在这条订阅下已经匹配成的集号。
    pub async fn page_resources(
        &self,
        req: &PageResourceRequest,
        page: usize,
        page_size: usize,
    ) -> Result<ResourcePage> {
        let mut qb = sqlx::QueryBuilder::new("SELECT r.info_hash, r.title, r.published_at");

        match req.sub_anime_id {
            Some(sub_anime_id) => {
                qb.push(
                    ", se.ep_num AS ep_num FROM resource r
                    LEFT JOIN sub_anime_episode se
                        ON se.resource_id = r.info_hash AND se.sub_anime_id = ",
                );
                qb.push_bind(sub_anime_id);
            }
            None => {
                qb.push(", NULL AS ep_num FROM resource r");
            }
        }

        qb.push(" WHERE 1=1");

        if let Some(start_at) = req.start_at {
            qb.push(" AND r.published_at >= ");
            qb.push_bind(start_at);
        }

        if let Some(end_at) = req.end_at {
            qb.push(" AND r.published_at <= ");
            qb.push_bind(end_at);
        }

        if let Some(keyword) = req
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|keyword| !keyword.is_empty())
        {
            qb.push(" AND (r.title LIKE ");
            qb.push_bind(format!("%{}%", keyword));
            qb.push(" OR r.match_title LIKE ");
            qb.push_bind(format!("%{}%", nfkc_to_lowercase(keyword)));
            qb.push(")");
        }

        // 多取一行判断还有没有下一页
        qb.push(" ORDER BY r.published_at DESC LIMIT ");
        qb.push_bind(page_size as i64 + 1);
        qb.push(" OFFSET ");
        qb.push_bind(page.saturating_sub(1).saturating_mul(page_size) as i64);

        let rows = qb.build().fetch_all(&self.pool).await?;

        let has_more = rows.len() > page_size;
        let mut data = Vec::with_capacity(rows.len().min(page_size));
        for row in rows.into_iter().take(page_size) {
            let info_hash: Vec<u8> = row.try_get("info_hash")?;
            let info_hash: [u8; 20] = info_hash
                .try_into()
                .map_err(|_| anyhow::anyhow!("info_hash length is not 20"))?;
            data.push(ResourceItem {
                info_hash: hex::encode(info_hash),
                title: row.try_get("title")?,
                published_at: row.try_get("published_at")?,
                ep_num: row.try_get("ep_num")?,
            });
        }

        Ok(ResourcePage {
            page,
            page_size,
            has_more,
            data,
        })
    }
}
