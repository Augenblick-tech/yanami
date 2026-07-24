use crate::entity::model::{AnimeBaseData, AnimeMetadata};

#[derive(Clone, Debug)]
pub struct AnimeEntity {
    data: AnimeBaseData,
}

impl AnimeEntity {
    pub(crate) fn new(data: AnimeBaseData) -> Self {
        Self { data }
    }

    pub fn id(&self) -> i64 {
        self.data.id
    }

    pub fn title(&self) -> Option<&str> {
        self.data
            .metadata
            .titles
            .iter()
            .find(|i| i.origin)
            .map(|i| i.name.as_str())
    }

    pub fn lock(&mut self) {
        self.data.lock = true;
    }

    pub fn unlock(&mut self) {
        self.data.lock = false;
    }

    pub fn metadata(&self) -> &AnimeMetadata {
        &self.data.metadata
    }

    pub(crate) fn is_locked(&self) -> bool {
        self.data.lock
    }

    pub fn update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.data.lock && !self.data.metadata.eq(metadata) {
            self.data.metadata = metadata.clone();
        }
    }

    pub fn force_update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.data.metadata.eq(metadata) {
            self.data.metadata = metadata.clone();
        }
    }
}
