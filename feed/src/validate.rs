use domain::{feed::FeedSource, shared::error::DomainError};

use crate::entity::FeedEntity;

impl FeedEntity {
    pub fn replace_source(&mut self, source: FeedSource) -> Result<(), DomainError> {
        validate_source(&source)?;
        self.source = source;
        Ok(())
    }
}

use crate::entity::validate_source;

#[cfg(test)]
mod tests {
    use domain::feed::FeedSourceId;

    use super::*;

    fn feed(id: &str, url: &str) -> FeedSource {
        FeedSource {
            id: FeedSourceId(id.to_string()),
            title: id.to_string(),
            site_url: Some(url.to_string()),
            search_url: None,
            source_key: None,
        }
    }

    #[test]
    fn replace_source_accepts_valid_source() {
        let mut entity = FeedEntity::new(feed("a", "https://a.example/rss")).expect("entity");

        entity
            .replace_source(feed("b", "https://b.example/rss"))
            .expect("replace source");

        assert_eq!(entity.read_data().id.0, "b");
    }

    #[test]
    fn replace_source_keeps_original_when_validation_fails() {
        let mut entity = FeedEntity::new(feed("a", "https://a.example/rss")).expect("entity");

        let error = entity
            .replace_source(feed("b", ""))
            .expect_err("empty site url must fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: feed source must have at least one of site url or search url"
        );
        assert_eq!(entity.read_data().id.0, "a");
    }
}
