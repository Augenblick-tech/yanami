use domain::anime::{AnimeId, AnimeMetadata};

#[derive(Clone, Debug)]
pub struct AnimeEntity {
    metadata: AnimeMetadata,
    lock: bool,
}

impl AnimeEntity {
    pub(crate) fn new(metadata: AnimeMetadata, lock: bool) -> Self {
        Self { metadata, lock }
    }

    pub fn id(&self) -> AnimeId {
        self.metadata.id
    }

    pub fn lock(&mut self) {
        self.lock = true;
    }

    pub fn unlock(&mut self) {
        self.lock = false;
    }

    pub fn update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.lock && !self.metadata.eq(metadata) {
            self.metadata = metadata.clone();
        }
    }

    pub fn force_update_metadata(&mut self, metadata: &AnimeMetadata) {
        if !self.metadata.eq(metadata) {
            self.metadata = metadata.clone();
        }
    }
}
