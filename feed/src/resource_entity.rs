use base32::Alphabet;
use domain::{
    feed::{Resource, ResourceId, ResourceSource},
    shared::error::DomainError,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use url::Url;

use crate::contracts::FetchedFeedItem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEntity {
    resource: Resource,
    sources: Vec<ResourceSource>,
}

impl ResourceEntity {
    pub fn new(resource: Resource, sources: Vec<ResourceSource>) -> Result<Self, DomainError> {
        validate_resource(&resource)?;
        let mut entity = Self {
            resource,
            sources: Vec::new(),
        };
        for source in sources {
            entity.include_source(source)?;
        }
        Ok(entity)
    }

    pub fn from_source(resource: Resource, source: ResourceSource) -> Result<Self, DomainError> {
        Self::new(resource, vec![source])
    }

    pub fn from_fetched_feed_item(
        source_key: &str,
        item: FetchedFeedItem,
        now: i64,
    ) -> Result<Self, DomainError> {
        let resource_id = resolve_resource_id(&item)?;
        let resource = Resource {
            id: resource_id.clone(),
            title: item.title,
            source_url: item.source_url.clone(),
            source_key: source_key.to_string(),
            published_at: item.published_at,
            created_at: now,
        };
        let source = ResourceSource {
            resource_id,
            source_key: source_key.to_string(),
            source_url: item.source_url,
            first_seen_at: now,
            last_seen_at: now,
        };
        Self::from_source(resource, source)
    }

    pub fn read_data(&self) -> &Resource {
        &self.resource
    }

    pub fn read_sources(&self) -> &[ResourceSource] {
        &self.sources
    }

    pub fn include_source(&mut self, source: ResourceSource) -> Result<bool, DomainError> {
        validate_source(&self.resource, &source)?;

        if let Some(existing) = self.sources.iter_mut().find(|existing| {
            existing.source_key == source.source_key && existing.source_url == source.source_url
        }) {
            let mut changed = false;
            if source.last_seen_at > existing.last_seen_at {
                existing.last_seen_at = source.last_seen_at;
                changed = true;
            }
            if source.first_seen_at < existing.first_seen_at {
                existing.first_seen_at = source.first_seen_at;
                changed = true;
            }
            return Ok(changed);
        }

        self.sources.push(source);
        self.sources.sort_by(|left, right| {
            left.source_key
                .cmp(&right.source_key)
                .then(left.source_url.cmp(&right.source_url))
        });
        Ok(true)
    }
}

fn resolve_resource_id(item: &FetchedFeedItem) -> Result<ResourceId, DomainError> {
    if let Some(info_hash) = magnet_info_hash(&item.source_url)? {
        return Ok(ResourceId(info_hash));
    }

    let bytes = item
        .torrent_content
        .as_deref()
        .ok_or(DomainError::InvariantViolation(
            "torrent content is missing",
        ))?;
    Ok(ResourceId(torrent_info_hash(bytes)?))
}

fn validate_resource(resource: &Resource) -> Result<(), DomainError> {
    if resource.id.0.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource id cannot be empty",
        ));
    }
    if resource.title.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource title cannot be empty",
        ));
    }
    if resource.source_url.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource source url cannot be empty",
        ));
    }
    if resource.source_key.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource source key cannot be empty",
        ));
    }
    Ok(())
}

fn torrent_info_hash(bytes: &[u8]) -> Result<String, DomainError> {
    let torrent: TorrentFile = serde_bencode::from_bytes(bytes)
        .map_err(|error| DomainError::external("torrent parse failed", error))?;
    let info = serde_bencode::to_bytes(&torrent.info)
        .map_err(|error| DomainError::external("torrent info encode failed", error))?;
    let mut hasher = Sha1::new();
    hasher.update(info);
    Ok(format!("{:x}", hasher.finalize()))
}

fn magnet_info_hash(source_url: &str) -> Result<Option<String>, DomainError> {
    let Ok(url) = Url::parse(source_url) else {
        return Ok(None);
    };
    if url.scheme() != "magnet" {
        return Ok(None);
    }

    let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "xt") else {
        return Ok(None);
    };
    let Some(hash) = value.strip_prefix("urn:btih:") else {
        return Ok(None);
    };

    if hash.len() <= 32 {
        let decoded = base32::decode(Alphabet::Rfc4648 { padding: true }, &hash.to_uppercase())
            .ok_or(DomainError::InvariantViolation("invalid base32 btih"))?;
        let mut result = String::new();
        for byte in decoded {
            result.push_str(&format!("{byte:02x}"));
        }
        return Ok(Some(result));
    }

    Ok(Some(hash.to_lowercase()))
}

#[derive(Debug, Deserialize)]
struct TorrentFile {
    info: TorrentInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct TorrentInfo(serde_bencode::value::Value);

fn validate_source(resource: &Resource, source: &ResourceSource) -> Result<(), DomainError> {
    if source.resource_id != resource.id {
        return Err(DomainError::InvariantViolation(
            "resource source id does not match resource",
        ));
    }
    if source.source_key.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource source key cannot be empty",
        ));
    }
    if source.source_url.trim().is_empty() {
        return Err(DomainError::InvariantViolation(
            "resource source url cannot be empty",
        ));
    }
    if source.last_seen_at < source.first_seen_at {
        return Err(DomainError::InvariantViolation(
            "resource source last seen cannot be earlier than first seen",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use domain::feed::{Resource, ResourceId, ResourceSource};

    use super::*;

    fn sample_resource() -> Resource {
        Resource {
            id: ResourceId("hash".to_string()),
            title: "title".to_string(),
            source_url: "magnet:?xt=urn:btih:hash".to_string(),
            source_key: "dmhy".to_string(),
            published_at: Some(10),
            created_at: 20,
        }
    }

    fn sample_source(url: &str, first_seen_at: i64, last_seen_at: i64) -> ResourceSource {
        ResourceSource {
            resource_id: ResourceId("hash".to_string()),
            source_key: "dmhy".to_string(),
            source_url: url.to_string(),
            first_seen_at,
            last_seen_at,
        }
    }

    fn sample_feed_item(source_url: &str, torrent_content: Option<Vec<u8>>) -> FetchedFeedItem {
        FetchedFeedItem {
            title: "title".to_string(),
            source_url: source_url.to_string(),
            torrent_content,
            published_at: Some(10),
        }
    }

    #[test]
    fn include_source_updates_existing_last_seen() {
        let mut entity = ResourceEntity::new(
            sample_resource(),
            vec![sample_source("magnet:?xt=urn:btih:hash", 20, 20)],
        )
        .expect("entity");

        let changed = entity
            .include_source(sample_source("magnet:?xt=urn:btih:hash", 20, 30))
            .expect("source");

        assert!(changed);
        assert_eq!(entity.read_sources()[0].last_seen_at, 30);
    }

    #[test]
    fn include_source_appends_new_source() {
        let mut entity = ResourceEntity::new(sample_resource(), vec![]).expect("entity");

        let changed = entity
            .include_source(sample_source("https://example.com/file.torrent", 20, 20))
            .expect("source");

        assert!(changed);
        assert_eq!(entity.read_sources().len(), 1);
    }

    #[test]
    fn builds_resource_id_from_magnet_info_hash() {
        let entity = ResourceEntity::from_fetched_feed_item(
            "dmhy",
            sample_feed_item(
                "magnet:?xt=urn:btih:0123456789ABCDEF0123456789ABCDEF01234567",
                None,
            ),
            20,
        )
        .expect("entity");

        assert_eq!(
            entity.read_data().id,
            ResourceId("0123456789abcdef0123456789abcdef01234567".to_string())
        );
        assert_eq!(entity.read_sources()[0].resource_id, entity.read_data().id);
    }

    #[test]
    fn parses_magnet_info_hash_from_base32() {
        let hash = magnet_info_hash("magnet:?xt=urn:btih:CI2FM6EQSCI2FM6EQSCI2FM6EQSCI2FM")
            .expect("base32 magnet");

        assert_eq!(hash.as_deref().map(str::len), Some(40));
        assert!(hash
            .as_deref()
            .is_some_and(|value| value.chars().all(|ch| ch.is_ascii_hexdigit())));
    }

    #[test]
    fn rejects_invalid_base32_btih() {
        let error = magnet_info_hash("magnet:?xt=urn:btih:INVALID*****")
            .expect_err("invalid base32 should fail");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: invalid base32 btih"
        );
    }

    #[test]
    fn builds_resource_id_from_torrent_content() {
        let entity = ResourceEntity::from_fetched_feed_item(
            "dmhy",
            sample_feed_item(
                "https://example.com/release.torrent",
                Some(b"d4:infod4:name4:testee".to_vec()),
            ),
            20,
        )
        .expect("entity");

        assert_eq!(entity.read_data().id.0.len(), 40);
        assert!(entity
            .read_data()
            .id
            .0
            .chars()
            .all(|ch| ch.is_ascii_hexdigit()));
    }
}
