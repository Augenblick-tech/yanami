use anime::source::AnimeSource;
use anyhow::{anyhow, Result};
use infra::{
    anime_source::{BangumiSource, YucSource},
    bangumi::BangumiClient,
    tmdb::TmdbClient,
    yuc::YucClient,
};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSourceKind {
    Bangumi,
    Yuc,
}

impl std::str::FromStr for MetadataSourceKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bangumi" => Ok(Self::Bangumi),
            "yuc" => Ok(Self::Yuc),
            other => Err(anyhow!("unsupported anime metadata source: {other}")),
        }
    }
}

pub fn normalize_sources(
    args_sources: Option<Vec<MetadataSourceKind>>,
    file_sources: Option<Vec<MetadataSourceKind>>,
    file_source: Option<MetadataSourceKind>,
) -> Vec<MetadataSourceKind> {
    let mut normalized = args_sources
        .or(file_sources)
        .unwrap_or_else(|| file_source.into_iter().collect());
    if normalized.is_empty() {
        normalized.push(MetadataSourceKind::Bangumi);
    }

    let mut unique = Vec::with_capacity(normalized.len());
    for source in normalized {
        if !unique.contains(&source) {
            unique.push(source);
        }
    }
    unique
}

pub fn build_metadata_sources(
    tmdb_token: &str,
    sources: &[MetadataSourceKind],
) -> Result<Vec<Box<dyn AnimeSource>>> {
    let tmdb = TmdbClient::new(tmdb_token)?;
    let mut metadata_sources: Vec<Box<dyn AnimeSource>> = Vec::with_capacity(sources.len());

    for source_kind in sources {
        match source_kind {
            MetadataSourceKind::Bangumi => {
                let source =
                    BangumiSource::new(BangumiClient::new()?, TmdbClient::new(tmdb_token)?);
                metadata_sources.push(Box::new(source));
            }
            MetadataSourceKind::Yuc => {
                let source = YucSource::new(
                    YucClient::new()?,
                    BangumiClient::new()?,
                    TmdbClient::new(tmdb_token)?,
                );
                metadata_sources.push(Box::new(source));
            }
        }
    }

    let _ = tmdb;
    Ok(metadata_sources)
}

pub fn normalize_sqlite_db_url(db_path: &str) -> String {
    if db_path.starts_with("sqlite:") {
        db_path.to_string()
    } else {
        format!("sqlite://{db_path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_sources_and_defaults_to_bangumi() {
        assert_eq!(
            normalize_sources(None, None, None),
            vec![MetadataSourceKind::Bangumi]
        );
        assert_eq!(
            normalize_sources(
                Some(vec![
                    MetadataSourceKind::Yuc,
                    MetadataSourceKind::Bangumi,
                    MetadataSourceKind::Yuc,
                ]),
                None,
                None
            ),
            vec![MetadataSourceKind::Yuc, MetadataSourceKind::Bangumi]
        );
    }

    #[test]
    fn normalizes_plain_db_path_into_sqlite_url() {
        assert_eq!(
            normalize_sqlite_db_url("/tmp/anime.sqlite"),
            "sqlite:///tmp/anime.sqlite"
        );
        assert_eq!(
            normalize_sqlite_db_url("sqlite::memory:"),
            "sqlite::memory:"
        );
    }

    #[test]
    fn parses_supported_metadata_source_kind() {
        assert_eq!(
            "bangumi".parse::<MetadataSourceKind>().expect("bangumi"),
            MetadataSourceKind::Bangumi
        );
        assert_eq!(
            "yuc".parse::<MetadataSourceKind>().expect("yuc"),
            MetadataSourceKind::Yuc
        );
        assert!("unknown".parse::<MetadataSourceKind>().is_err());
    }

    #[test]
    fn falls_back_to_legacy_single_source_when_needed() {
        assert_eq!(
            normalize_sources(None, None, Some(MetadataSourceKind::Yuc)),
            vec![MetadataSourceKind::Yuc]
        );
    }

    #[test]
    fn build_metadata_sources_requires_valid_tmdb_token() {
        let error = build_metadata_sources("bad\nkey", &[MetadataSourceKind::Bangumi])
            .err()
            .expect("empty tmdb token must fail");
        assert!(error.to_string().contains("invalid tmdb key"));
    }

    #[test]
    fn build_metadata_sources_creates_requested_sources() {
        let sources = build_metadata_sources(
            "test-token",
            &[MetadataSourceKind::Bangumi, MetadataSourceKind::Yuc],
        )
        .expect("build metadata sources");

        assert_eq!(sources.len(), 2);
    }
}
