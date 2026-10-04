use std::path::PathBuf;
use std::sync::Arc;

use common::shared::{cap::SeriesLocation, error::Error, str::to_safe_filename};

use crate::entity::{anime_entity::AnimeEntity, cap::AnimeRepository, model::AnimeSeriesMetadata};

/// 一个 TMDB 剧集下的季度集合。
///
/// 系列没有独立的个体实体，它就是同属一个 TMDB 剧集 id 的季度条目集合；
/// 只持范围 id 与仓储，成员与系列展示信息都按需取回，不揣实体。
#[derive(Clone)]
pub struct AnimeSeries {
    tmdb_id: i64,
    repo: Arc<dyn AnimeRepository>,
}

impl AnimeSeries {
    pub(super) fn new(tmdb_id: i64, repo: Arc<dyn AnimeRepository>) -> Self {
        Self { tmdb_id, repo }
    }

    /// 系列身份，TMDB 剧集 id。
    pub fn tmdb_id(&self) -> i64 {
        self.tmdb_id
    }

    /// 系列下的季度条目，按季号升序。
    pub async fn list(&self) -> Result<Vec<AnimeEntity>, Error> {
        let animes = self
            .repo
            .list_by_series(self.tmdb_id)
            .await
            .map_err(|e| Error::external("anime series list failed", e))?;

        let mut animes: Vec<AnimeEntity> = animes
            .into_iter()
            .map(|i| AnimeEntity::new(i.data))
            .collect();

        // 季度按季号排，季号未知的排最后，同季号按 id 保证稳定
        animes.sort_by_key(|i| (i.season_number().is_none(), i.season_number(), i.id()));

        Ok(animes)
    }

    /// 系列展示信息，一个剧集只有一份。
    pub async fn metadata(&self) -> Result<AnimeSeriesMetadata, Error> {
        self.repo
            .find_series(self.tmdb_id)
            .await
            .map_err(|e| Error::external("anime series metadata failed", e))?
            .ok_or_else(|| Error::not_found("anime series metadata not found"))
    }

    /// 系列在媒体库里的目录名。
    ///
    /// 目录名只能是一个普通分量：空名字、`.`、`..` 都会让落点跑出系列目录，这里报错，
    /// 让剧集保持待下载，不猜路径。
    pub async fn folder_name(&self) -> Result<String, Error> {
        let name = to_safe_filename(&self.metadata().await?.origin_name);
        if name.is_empty() {
            return Err(Error::invariant("anime series folder name is empty"));
        }
        if name == "." || name == ".." {
            return Err(Error::invariant(
                "anime series folder name must not be . or ..",
            ));
        }
        Ok(name)
    }
}

#[async_trait::async_trait]
impl SeriesLocation for AnimeSeries {
    /// 某一季在媒体库里的落点：系列目录 / S{季号}，季号以 TMDB 为唯一出处。
    async fn location_of(&self, anime_id: i64) -> Result<PathBuf, Error> {
        let prop = self
            .repo
            .find(anime_id)
            .await
            .map_err(|e| Error::external("anime series season find failed", e))?
            .ok_or_else(|| Error::not_found("anime series season not found"))?;
        let anime = AnimeEntity::new(prop.data);

        // 落点必须是本系列的季度，别的系列的条目一概不认
        if anime.series_id() != Some(self.tmdb_id) {
            return Err(Error::invariant(
                "anime series season belongs to another series",
            ));
        }

        let season = anime
            .season_number()
            .ok_or_else(|| Error::not_found("anime series season number not found"))?;
        Ok(PathBuf::from(self.folder_name().await?).join(format!("S{:02}", season)))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;
    use crate::entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeMetadata, AnimeSeason,
        AnimeSourceTarget, AnimeTitle,
    };
    use crate::infra::repository::client::AnimeSqliteClient;

    const DB_FILE: &str = "anime_series_test.db";
    const TMDB_ID: i64 = 286346;
    const BANGUMI_ID: i64 = 558064;

    /// 建一个真实的临时库并接上生产实现，TempDir 由调用方持有以免被清理。
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

    fn series_metadata(origin_name: &str) -> AnimeSeriesMetadata {
        AnimeSeriesMetadata {
            origin_name: origin_name.to_string(),
            cn_name: "杂役女仆".to_string(),
            desc: String::new(),
            air_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            genres: vec![],
        }
    }

    fn metadata(
        tmdb_id: Option<i64>,
        season: Option<u32>,
        series: Option<AnimeSeriesMetadata>,
    ) -> AnimeMetadata {
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

        let season = season
            .map(|number| AnimeSeason {
                target: AnimeSourceTarget::TMDB,
                lang: AnimeLangTarget::JP,
                desc: String::new(),
                season: number,
                eps: vec![],
                planned_episode_count: 12,
            })
            .into_iter()
            .collect();

        AnimeMetadata {
            series_metadata: series,
            external_link,
            titles: vec![AnimeTitle {
                name: "テスト".to_string(),
                match_name: "テスト".to_string(),
                target: AnimeLangTarget::JP,
                origin: true,
            }],
            air_weekday: AnimeAirWeekday::Friday,
            air_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            air_quarter: 202607,
            season,
        }
    }

    async fn insert(client: &AnimeSqliteClient, metadata: &AnimeMetadata) -> i64 {
        AnimeRepository::insert(client, metadata, false)
            .await
            .unwrap()
            .data
            .id
    }

    fn repo(client: &AnimeSqliteClient) -> Arc<dyn AnimeRepository> {
        Arc::new(client.clone())
    }

    #[tokio::test]
    async fn location_of_joins_series_folder_and_season() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let anime_id = insert(
            &client,
            &metadata(
                Some(TMDB_ID),
                Some(2),
                Some(series_metadata("All-Works Maid")),
            ),
        )
        .await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let location = series.location_of(anime_id).await.unwrap();
        assert_eq!(location, PathBuf::from("All-Works Maid").join("S02"));
        assert_eq!(location.components().count(), 2);
    }

    #[tokio::test]
    async fn location_of_rejects_season_of_another_series() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let anime_id = insert(
            &client,
            &metadata(
                Some(TMDB_ID),
                Some(1),
                Some(series_metadata("All-Works Maid")),
            ),
        )
        .await;

        let other_series = AnimeSeries::new(TMDB_ID + 1, repo(&client));
        let err = other_series.location_of(anime_id).await.unwrap_err();
        assert!(matches!(
            err,
            Error::InvariantViolation(ref m) if m == "anime series season belongs to another series"
        ));
    }

    #[tokio::test]
    async fn location_of_not_found_when_season_number_missing() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let anime_id = insert(
            &client,
            &metadata(Some(TMDB_ID), None, Some(series_metadata("All-Works Maid"))),
        )
        .await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let err = series.location_of(anime_id).await.unwrap_err();
        assert!(matches!(
            err,
            Error::NotFound(ref m) if m == "anime series season number not found"
        ));
    }

    #[tokio::test]
    async fn location_of_not_found_when_series_row_missing() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        // 有 TMDB 身份也有季号，但没有带回系列展示信息，anime_series 里没有对应行
        let anime_id = insert(&client, &metadata(Some(TMDB_ID), Some(1), None)).await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let err = series.location_of(anime_id).await.unwrap_err();
        assert!(matches!(
            err,
            Error::NotFound(ref m) if m == "anime series metadata not found"
        ));
    }

    #[tokio::test]
    async fn list_orders_known_seasons_ascending_then_unknown_last() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let season_two = insert(
            &client,
            &metadata(
                Some(TMDB_ID),
                Some(2),
                Some(series_metadata("All-Works Maid")),
            ),
        )
        .await;
        let season_one = insert(
            &client,
            &metadata(
                Some(TMDB_ID),
                Some(1),
                Some(series_metadata("All-Works Maid")),
            ),
        )
        .await;
        let unknown_season = insert(
            &client,
            &metadata(Some(TMDB_ID), None, Some(series_metadata("All-Works Maid"))),
        )
        .await;
        let season_one_later = insert(
            &client,
            &metadata(
                Some(TMDB_ID),
                Some(1),
                Some(series_metadata("All-Works Maid")),
            ),
        )
        .await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let ids: Vec<i64> = series
            .list()
            .await
            .unwrap()
            .iter()
            .map(|i| i.id())
            .collect();

        // 生产排序键是 (季号未知, 季号, id)：已知季号在前按季号升序，未知季号排最后。
        // 任务描述写的“无季号排最前”与实现不符，这里按真实行为断言。
        assert_eq!(
            ids,
            vec![season_one, season_one_later, season_two, unknown_season]
        );
    }

    #[tokio::test]
    async fn folder_name_sanitizes_series_origin_name() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        let origin_name = "Fate/stay night: Unlimited Blade Works?";
        insert(
            &client,
            &metadata(Some(TMDB_ID), Some(1), Some(series_metadata(origin_name))),
        )
        .await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let folder = series.folder_name().await.unwrap();
        assert_eq!(folder, "Fate stay night Unlimited Blade Works");
        assert_eq!(folder, to_safe_filename(origin_name));
    }

    #[tokio::test]
    async fn location_of_keeps_a_slash_in_the_series_name_inside_one_component() {
        let (dir, client) = setup().await;
        assert!(dir.path().join(DB_FILE).is_file());

        // 乱马1/2：斜杠属于目录名，换成空格后落点仍是两层，不会多出一层
        let anime_id = insert(
            &client,
            &metadata(Some(TMDB_ID), Some(1), Some(series_metadata("乱马1/2"))),
        )
        .await;

        let series = AnimeSeries::new(TMDB_ID, repo(&client));
        let location = series.location_of(anime_id).await.unwrap();

        assert_eq!(location, PathBuf::from("乱马1 2").join("S01"));
        assert_eq!(
            location.components().count(),
            2,
            "actual location: {}",
            location.display()
        );
    }

    #[tokio::test]
    async fn location_of_refuses_series_name_that_is_not_a_directory_name() {
        for (origin_name, expected) in [
            ("", "anime series folder name is empty"),
            ("///", "anime series folder name is empty"),
            (".", "anime series folder name must not be . or .."),
            ("..", "anime series folder name must not be . or .."),
        ] {
            let (dir, client) = setup().await;
            assert!(dir.path().join(DB_FILE).is_file());

            let anime_id = insert(
                &client,
                &metadata(Some(TMDB_ID), Some(1), Some(series_metadata(origin_name))),
            )
            .await;

            let series = AnimeSeries::new(TMDB_ID, repo(&client));
            let err = series.location_of(anime_id).await.unwrap_err();
            assert!(
                matches!(err, Error::InvariantViolation(ref m) if m == expected),
                "origin_name {origin_name:?} actual error: {err}"
            );
        }
    }
}
