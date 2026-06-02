use domain::{
    anime::capability::{AnimeLockCap, AnimeMetadataUpdateCap},
    anime::{AnimeId, AnimeMetadata},
    shared::error::DomainError,
};
use std::fmt;

use crate::repository::AnimeSnapshot;

#[path = "tracking.rs"]
mod tracking;

#[derive(Clone)]
pub struct AnimeEntity {
    anime: AnimeSnapshot,
}

impl fmt::Debug for AnimeEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnimeEntity")
            .field("id", &self.anime.metadata.id)
            .finish()
    }
}

impl AnimeEntity {
    /// 基于元数据快照构造番剧聚合根。
    pub fn new(anime: AnimeSnapshot) -> Result<Self, DomainError> {
        validate_metadata(&anime.metadata)?;
        Ok(Self { anime })
    }

    pub fn from_metadata(metadata: AnimeMetadata) -> Result<Self, DomainError> {
        Self::new(AnimeSnapshot {
            metadata,
            metadata_locked: false,
        })
    }

    pub fn id(&self) -> AnimeId {
        self.anime.metadata.id
    }

    pub fn read_data(&self) -> &AnimeSnapshot {
        &self.anime
    }

    pub fn into_snapshot(self) -> AnimeSnapshot {
        self.anime
    }

    pub async fn set_metadata_locked(
        &mut self,
        locker: &dyn AnimeLockCap,
        locked: bool,
    ) -> Result<(), DomainError> {
        if self.anime.metadata_locked == locked {
            return Ok(());
        }
        locker
            .write_lock_status(self.anime.metadata.id, locked)
            .await?;
        self.anime.metadata_locked = locked;
        Ok(())
    }

    pub async fn update_metadata(
        &mut self,
        writer: &dyn AnimeMetadataUpdateCap,
        new_metadata: AnimeMetadata,
    ) -> Result<(), DomainError> {
        validate_metadata(&new_metadata)?;
        writer
            .update_metadata(self.anime.metadata.id, &new_metadata)
            .await?;
        self.anime.metadata = new_metadata;
        Ok(())
    }
}

pub fn validate_metadata(metadata: &AnimeMetadata) -> Result<(), DomainError> {
    use chrono::NaiveDate;

    if metadata.id.0 <= 0 {
        return Err(DomainError::InvariantViolation("anime id must be positive"));
    }
    if metadata.titles.original_ja.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "anime title original_ja cannot be empty",
        ));
    }
    if metadata.planned_episode_count.0 <= 0 {
        return Err(DomainError::InvariantViolation(
            "planned episode count must be positive",
        ));
    }
    if metadata.season.0 <= 0 {
        return Err(DomainError::InvariantViolation(
            "season number must be positive",
        ));
    }
    if NaiveDate::parse_from_str(&metadata.air_date.0, "%Y-%m-%d").is_err() {
        return Err(DomainError::InvariantViolation(
            "air date must be yyyy-mm-dd",
        ));
    }

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use async_trait::async_trait;
    use domain::anime::capability::{AnimeLockCap, AnimeMetadataUpdateCap};
    use domain::anime::AnimeId;
    use domain::anime::{
        AirDate, AnimeMetadata, AnimeTitleSet, BroadcastWeekday, PlannedEpisodeCount, SeasonNumber,
    };
    use domain::shared::error::DomainError;

    use super::*;

    pub(crate) fn sample_metadata() -> AnimeMetadata {
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

    pub(crate) fn sample_item() -> crate::repository::AnimeSnapshot {
        crate::repository::AnimeSnapshot {
            metadata: sample_metadata(),
            metadata_locked: false,
        }
    }

    #[test]
    fn read_data_returns_current_snapshot() {
        let item = sample_item();
        let entity = AnimeEntity::new(item).expect("entity");

        assert_eq!(entity.read_data().metadata.id, AnimeId(7));
    }

    struct NoopLocker;
    #[async_trait]
    impl AnimeLockCap for NoopLocker {
        async fn write_lock_status(
            &self,
            _anime_id: AnimeId,
            _locked: bool,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn set_metadata_locked_updates_state() {
        let item = sample_item();
        let mut entity = AnimeEntity::new(item).expect("entity");

        entity
            .set_metadata_locked(&NoopLocker, true)
            .await
            .expect("set metadata locked");

        assert!(entity.read_data().metadata_locked);
    }

    struct RecordingUpdater;
    #[async_trait]
    impl AnimeMetadataUpdateCap for RecordingUpdater {
        async fn update_metadata(
            &self,
            _anime_id: AnimeId,
            _metadata: &AnimeMetadata,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn update_metadata_validates_and_persists() {
        let mut entity = AnimeEntity::new(sample_item()).expect("entity");
        let mut new_meta = sample_metadata();
        new_meta.titles.original_ja = "新しいタイトル".to_string();

        entity
            .update_metadata(&RecordingUpdater, new_meta.clone())
            .await
            .expect("update_metadata");
        assert_eq!(
            entity.read_data().metadata.titles.original_ja,
            "新しいタイトル"
        );
    }

    #[tokio::test]
    async fn update_metadata_rejects_invalid_data() {
        let mut entity = AnimeEntity::new(sample_item()).expect("entity");
        let invalid = AnimeMetadata {
            planned_episode_count: PlannedEpisodeCount(0),
            ..sample_metadata()
        };
        assert!(entity
            .update_metadata(&RecordingUpdater, invalid)
            .await
            .is_err());
    }
}
