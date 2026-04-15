use std::sync::Arc;

use domain::{
    anime::{
        AnimeId, AnimeListQuery, AnimeMetadata, AnimeMetadataRepository, AnimeStateRepository,
    },
    shared::error::DomainError,
};

use domain::anime::capability::{AnimeLockCap, AnimeMetadataUpdateCap};
use crate::entity::AnimeEntity;
use crate::repository::{AnimeRepository, AnimeSnapshot};

#[derive(Clone)]
pub struct AnimeCaps {
    pub locker: Arc<dyn AnimeLockCap>,
    pub metadata_updater: Arc<dyn AnimeMetadataUpdateCap>,
}

#[derive(Clone)]
pub struct Animes {
    pub caps: AnimeCaps,
    metadata_repository: Arc<dyn AnimeMetadataRepository>,
    state_repository: Arc<dyn AnimeStateRepository>,
    anime_repository: Arc<dyn AnimeRepository>,
}

impl Animes {
    /// 构造番剧聚合根集合入口。
    pub fn new(
        caps: AnimeCaps,
        metadata_repository: Arc<dyn AnimeMetadataRepository>,
        state_repository: Arc<dyn AnimeStateRepository>,
        anime_repository: Arc<dyn AnimeRepository>,
    ) -> Self {
        Self {
            caps,
            metadata_repository,
            state_repository,
            anime_repository,
        }
    }

    fn build_entity(&self, anime: AnimeSnapshot) -> Result<AnimeEntity, DomainError> {
        AnimeEntity::new(anime)
    }

    pub async fn load(&self, anime_id: AnimeId) -> Result<AnimeEntity, DomainError> {
        let anime = self
            .anime_repository
            .find(anime_id)
            .await?
            .ok_or(DomainError::InvariantViolation("anime not found"))?;
        self.build_entity(anime)
    }

    pub async fn list(&self, query: AnimeListQuery) -> Result<Vec<AnimeEntity>, DomainError> {
        self.anime_repository
            .list(query)
            .await?
            .into_iter()
            .map(|anime| self.build_entity(anime))
            .collect()
    }

    pub async fn load_many(&self, anime_ids: &[AnimeId]) -> Result<Vec<AnimeEntity>, DomainError> {
        self.anime_repository
            .list_by_ids(anime_ids)
            .await?
            .into_iter()
            .map(|anime| self.build_entity(anime))
            .collect()
    }

    pub async fn save(&self, entity: &AnimeEntity) -> Result<(), DomainError> {
        let anime = entity.read_data();
        self.state_repository
            .set_metadata_locked(anime.metadata.id, anime.metadata_locked)
            .await
    }

    pub async fn save_list(
        &self,
        entities: &[AnimeEntity],
    ) -> Result<Vec<AnimeEntity>, DomainError> {
        let entries = entities
            .iter()
            .filter(|entity| !entity.read_data().metadata_locked)
            .map(|entity| entity.read_data().metadata.clone())
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let replaced = self
            .metadata_repository
            .replace_anime_metadata(&entries)
            .await?;
        self.load_many(&replaced.new_anime_ids).await
    }

    pub async fn update_metadata(&self, entity: &AnimeEntity) -> Result<(), DomainError> {
        let anime = entity.read_data();
        self.caps
            .metadata_updater
            .update_metadata(anime.metadata.id, &anime.metadata)
            .await
    }

    pub async fn create(&self, metadata: AnimeMetadata) -> Result<AnimeEntity, DomainError> {
        let entity = AnimeEntity::from_metadata(metadata)?;
        self.metadata_repository
            .create_anime_metadata(&entity.read_data().metadata)
            .await?;
        Ok(entity)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use domain::anime::{
        AirDate, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount, SeasonNumber,
        capability::{AnimeLockCap, AnimeMetadataUpdateCap}, AnimeId,
    };

    use super::*;
    use crate::repository::AnimeSnapshot;

    #[derive(Default)]
    struct RecordingMetadataRepository {
        created: Mutex<Vec<AnimeMetadata>>,
        replaced: Mutex<Vec<Vec<AnimeMetadata>>>,
    }

    #[async_trait]
    impl AnimeMetadataRepository for RecordingMetadataRepository {
        async fn create_anime_metadata(&self, metadata: &AnimeMetadata) -> Result<(), DomainError> {
            self.created.lock().expect("created").push(metadata.clone());
            Ok(())
        }

        async fn replace_anime_metadata(
            &self,
            entries: &[AnimeMetadata],
        ) -> Result<domain::anime::ReplaceAnimeMetadataResult, DomainError> {
            self.replaced
                .lock()
                .expect("replaced")
                .push(entries.to_vec());
            Ok(domain::anime::ReplaceAnimeMetadataResult {
                new_anime_ids: entries.iter().map(|entry| entry.id).collect(),
            })
        }
    }

    struct NoopAnimeStateRepository;

    #[async_trait]
    impl AnimeStateRepository for NoopAnimeStateRepository {
        async fn set_metadata_locked(
            &self,
            _anime_id: AnimeId,
            _locked: bool,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct StubQuery {
        items: Mutex<Vec<AnimeSnapshot>>,
    }

    #[async_trait]
    impl AnimeRepository for StubQuery {
        async fn list(&self, _query: AnimeListQuery) -> Result<Vec<AnimeSnapshot>, DomainError> {
            Ok(self.items.lock().expect("items").clone())
        }

        async fn find(&self, anime_id: AnimeId) -> Result<Option<AnimeSnapshot>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("items")
                .iter()
                .find(|item| item.metadata.id == anime_id)
                .cloned())
        }

        async fn list_by_ids(
            &self,
            anime_ids: &[AnimeId],
        ) -> Result<Vec<AnimeSnapshot>, DomainError> {
            Ok(self
                .items
                .lock()
                .expect("items")
                .iter()
                .filter(|item| anime_ids.contains(&item.metadata.id))
                .cloned()
                .collect())
        }
    }

    fn sample_metadata() -> AnimeMetadata {
        AnimeMetadata {
            id: AnimeId(7),
            titles: AnimeTitleSet {
                original_ja: "葬送のフリーレン".to_string(),
                localized_zh_cn: "葬送的芙莉莲".to_string(),
                localized_zh_tw: "葬送的芙蓮".to_string(),
                search_name: "Frieren".to_string(),
                aliases: vec!["Sousou no Frieren".to_string()],
            },
            broadcast_weekday: BroadcastWeekday(5),
            planned_episode_count: PlannedEpisodeCount(12),
            air_date: AirDate("2026-04-01".to_string()),
            season: SeasonNumber(1),
        }
    }

    struct NoopLockerCaps;
    #[async_trait]
    impl AnimeLockCap for NoopLockerCaps {
        async fn write_lock_status(&self, _anime_id: AnimeId, _locked: bool) -> Result<(), DomainError> {
            Ok(())
        }
    }

    struct NoopMetadataUpdater;
    #[async_trait]
    impl AnimeMetadataUpdateCap for NoopMetadataUpdater {
        async fn update_metadata(&self, _anime_id: AnimeId, _metadata: &AnimeMetadata) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn test_caps() -> AnimeCaps {
        AnimeCaps {
            locker: Arc::new(NoopLockerCaps),
            metadata_updater: Arc::new(NoopMetadataUpdater),
        }
    }

    #[tokio::test]
    async fn create_persists_metadata_and_returns_entity() {
        let metadata_repository = Arc::new(RecordingMetadataRepository::default());
        let query = Arc::new(StubQuery::default());
        let animes = Animes::new(
            test_caps(),
            metadata_repository.clone(),
            Arc::new(NoopAnimeStateRepository),
            query,
        );

        let entity = animes.create(sample_metadata()).await.expect("create");

        assert_eq!(entity.read_data().metadata.id, AnimeId(7));
        assert_eq!(
            metadata_repository.created.lock().expect("created").len(),
            1
        );
    }

    #[tokio::test]
    async fn list_returns_entity_list() {
        let query = Arc::new(StubQuery::default());
        query.items.lock().expect("items").push(AnimeSnapshot {
            metadata: sample_metadata(),
            metadata_locked: false,
        });
        let animes = Animes::new(
            test_caps(),
            Arc::new(RecordingMetadataRepository::default()),
            Arc::new(NoopAnimeStateRepository),
            query.clone(),
        );

        let items = animes
            .list(AnimeListQuery {
                ..Default::default()
            })
            .await
            .expect("list");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].read_data().metadata.id, AnimeId(7));
    }

    #[tokio::test]
    async fn load_returns_entity_from_repository() {
        let query = Arc::new(StubQuery::default());
        query.items.lock().expect("items").push(AnimeSnapshot {
            metadata: sample_metadata(),
            metadata_locked: false,
        });
        let animes = Animes::new(
            test_caps(),
            Arc::new(RecordingMetadataRepository::default()),
            Arc::new(NoopAnimeStateRepository),
            query.clone(),
        );

        let entity = animes.load(AnimeId(7)).await.expect("load");
        assert_eq!(entity.read_data().metadata.id, AnimeId(7));
    }
}
