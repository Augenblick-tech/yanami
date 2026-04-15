#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use domain::anime::capability::AnimeLockCap;
    use domain::anime::AnimeId;
    use domain::shared::error::DomainError;

    use crate::entity::{tests::sample_item, AnimeEntity};

    struct NoopLocker;
    #[async_trait]
    impl AnimeLockCap for NoopLocker {
        async fn write_lock_status(&self, _anime_id: AnimeId, _locked: bool) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn set_metadata_locked_updates_current_state() {
        let item = sample_item();
        let mut entity = AnimeEntity::new(item).expect("entity");

        entity
            .set_metadata_locked(&NoopLocker, true)
            .await
            .expect("set metadata locked");

        assert!(entity.read_data().metadata_locked);
    }
}
