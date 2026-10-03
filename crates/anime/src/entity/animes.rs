use std::{ops::ControlFlow, sync::Arc};

use common::shared::error::Error;

use crate::entity::{
    anime_entity::AnimeEntity,
    cap::AnimeRepository,
    model::{AnimeBaseData, AnimeListQuery, AnimeMetadata, AnimeProps},
    series::AnimeSeries,
};

#[derive(Clone)]
pub struct Animes {
    repo: Arc<dyn AnimeRepository>,
}

impl Animes {
    pub fn new(repo: Arc<dyn AnimeRepository>) -> Self {
        Self { repo }
    }

    pub async fn list_by_ids(&self, ids: Vec<i64>) -> Result<Vec<AnimeEntity>, Error> {
        let props = self
            .repo
            .list_by_ids(&ids)
            .await
            .map_err(|e| Error::external("animes get list by anime ids failed", e))?;
        Ok(props
            .into_iter()
            .map(|i| AnimeEntity::new(i.data))
            .collect())
    }

    pub async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeEntity>, Error> {
        let list = self
            .repo
            .list(&query)
            .await
            .map_err(|e| Error::external("animes list anime_entity failed", e))?;
        Ok(list.into_iter().map(|i| AnimeEntity::new(i.data)).collect())
    }

    pub async fn range<F>(&self, query: &AnimeListQuery, mut f: F) -> Result<(), Error>
    where
        F: FnMut(AnimeEntity) -> anyhow::Result<ControlFlow<()>> + Send + 'static,
    {
        self.repo
            .range(query, &mut |props: AnimeProps| {
                f(AnimeEntity::new(props.data))
            })
            .await
            .map_err(|e| Error::external("animes range anime_entity failed", e))
    }

    pub async fn get(&self, anime_id: i64) -> Result<Option<AnimeEntity>, Error> {
        let props = self
            .repo
            .find(anime_id)
            .await
            .map_err(|e| Error::external("anims get anime_entity failed", e))?;
        if let Some(props) = props {
            Ok(Some(AnimeEntity::new(props.data)))
        } else {
            Ok(None)
        }
    }

    /// 创建一部番剧：锁状态随创建在同一个事务里落库，创建之后不需要再写一次。
    pub async fn create(&self, metadata: AnimeMetadata, lock: bool) -> Result<AnimeEntity, Error> {
        let props = self
            .repo
            .insert(&metadata, lock)
            .await
            .map_err(|e| Error::external("animes create anime_entity failed", e))?;
        Ok(AnimeEntity::new(props.data))
    }

    pub async fn save(&self, entity: &AnimeEntity) -> Result<(), Error> {
        self.repo
            .update(&AnimeBaseData {
                id: entity.id(),
                metadata: entity.metadata().clone(),
                lock: entity.is_locked(),
            })
            .await
            .map_err(|e| Error::external("animes save anime_entity failed", e))
    }

    pub async fn sync_metadata(
        &self,
        metadata: Vec<AnimeMetadata>,
    ) -> Result<Vec<AnimeEntity>, Error> {
        if metadata.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .repo
            .sync_metadata_with_not_lock(metadata)
            .await
            .map_err(|e| Error::external("animes sync anime metadata failed", e))?
            .into_iter()
            .map(|i| AnimeEntity::new(i.data))
            .collect())
    }
}

impl Animes {
    /// 按 id 取番剧条目所属的季度集合。
    pub async fn series(&self, anime_id: i64) -> Result<AnimeSeries, Error> {
        let anime = self
            .get(anime_id)
            .await?
            .ok_or_else(|| Error::not_found("animes get series anime not found"))?;
        self.series_of(&anime)
    }

    /// 取番剧条目所属的季度集合，没有 TMDB 身份的条目没有系列。
    pub fn series_of(&self, anime: &AnimeEntity) -> Result<AnimeSeries, Error> {
        let tmdb_id = anime
            .series_id()
            .ok_or_else(|| Error::not_found("animes series not found tmdb id"))?;
        Ok(AnimeSeries::new(tmdb_id, self.repo.clone()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;
    use crate::entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeSeason, AnimeSeriesMetadata,
        AnimeSourceTarget, AnimeTitle,
    };
    use crate::infra::repository::client::AnimeSqliteClient;

    const DB_FILE: &str = "animes_test.db";
    const TMDB_ID: i64 = 286346;
    const BANGUMI_ID: i64 = 558064;

    /// 建一个真实的 SQLite 临时库并跑一遍生产建表逻辑，TempDir 由调用方持有以免被清理。
    async fn setup() -> (tempfile::TempDir, AnimeSqliteClient) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DB_FILE);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        let client = AnimeSqliteClient::new(pool);
        client.init().await.unwrap();
        (dir, client)
    }

    fn metadata(tmdb_id: Option<i64>) -> AnimeMetadata {
        let mut external_link = vec![AnimeEx {
            id: AnimeIdType::Int(BANGUMI_ID),
            target: AnimeSourceTarget::Bangumi,
            r#type: Some("tv".to_string()),
        }];
        if let Some(tmdb_id) = tmdb_id {
            external_link.push(AnimeEx {
                id: AnimeIdType::Int(tmdb_id),
                target: AnimeSourceTarget::TMDB,
                r#type: Some("tv".to_string()),
            });
        }

        AnimeMetadata {
            series_metadata: Some(AnimeSeriesMetadata {
                origin_name: "All-Works Maid".to_string(),
                cn_name: "杂役女仆".to_string(),
                desc: String::new(),
                air_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                genres: vec![],
            }),
            external_link,
            titles: vec![AnimeTitle {
                name: "雑役メイド".to_string(),
                match_name: "雑役メイド".to_string(),
                target: AnimeLangTarget::JP,
                origin: true,
            }],
            air_weekday: AnimeAirWeekday::Wednesday,
            air_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            air_quarter: 202607,
            season: vec![AnimeSeason {
                target: AnimeSourceTarget::TMDB,
                lang: AnimeLangTarget::JP,
                desc: String::new(),
                season: 1,
                eps: vec![],
                planned_episode_count: 12,
            }],
        }
    }

    fn animes(client: &AnimeSqliteClient) -> Animes {
        Animes::new(Arc::new(client.clone()) as Arc<dyn AnimeRepository>)
    }

    #[tokio::test]
    async fn series_of_returns_series_of_tmdb_identity() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let animes = animes(&client);
        let entity = animes.create(metadata(Some(TMDB_ID)), false).await.unwrap();
        assert_eq!(entity.series_id(), Some(TMDB_ID));

        let series = animes.series_of(&entity).unwrap();
        assert_eq!(series.tmdb_id(), TMDB_ID);

        // 按 id 走真实查询路径也应拿到同一个系列
        let series = animes.series(entity.id()).await.unwrap();
        assert_eq!(series.tmdb_id(), TMDB_ID);
    }

    #[tokio::test]
    async fn series_of_not_found_without_tmdb_identity() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let animes = animes(&client);
        let entity = animes.create(metadata(None), false).await.unwrap();
        assert_eq!(entity.series_id(), None);

        let err = animes.series_of(&entity).err().expect("should fail");
        assert!(matches!(
            err,
            Error::NotFound(ref m) if m == "animes series not found tmdb id"
        ));

        // 库里已有条目但没有 TMDB 身份，同样拿不到系列
        let err = animes.series(entity.id()).await.err().expect("should fail");
        assert!(matches!(
            err,
            Error::NotFound(ref m) if m == "animes series not found tmdb id"
        ));
    }

    #[tokio::test]
    async fn series_not_found_for_missing_anime() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let animes = animes(&client);
        let err = animes
            .series(999_999)
            .await
            .err()
            .expect("should not find anime");
        assert!(matches!(
            err,
            Error::NotFound(ref m) if m == "animes get series anime not found"
        ));
    }

    #[tokio::test]
    async fn create_writes_lock_state_in_one_write() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let animes = animes(&client);
        let locked = animes.create(metadata(Some(TMDB_ID)), true).await.unwrap();
        assert!(locked.is_locked(), "created entity should carry the lock");
        let stored = animes
            .get(locked.id())
            .await
            .unwrap()
            .expect("created anime should be found");
        assert!(stored.is_locked(), "lock should be persisted by the create");

        let unlocked = animes.create(metadata(None), false).await.unwrap();
        assert!(!unlocked.is_locked());
        let stored = animes
            .get(unlocked.id())
            .await
            .unwrap()
            .expect("created anime should be found");
        assert!(!stored.is_locked());
    }
}
