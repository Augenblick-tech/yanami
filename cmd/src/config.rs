use std::fs;

use anyhow::{anyhow, Result};
use clap::Parser;
use job::{config::ScheduledJobConfig, StrictScheduledJobConfigFactory};
use serde::Deserialize;

use crate::metadata::{normalize_sources, MetadataSourceKind};

#[derive(Parser, Debug, Default)]
pub struct SchedulerArgs {
    #[clap(short, long, env)]
    config: Option<String>,
    #[clap(short, long, env)]
    addr: Option<String>,
    #[clap(short, long, env)]
    db_path: Option<String>,
    #[clap(short, long, env)]
    key: Option<String>,
    #[clap(short, long, env)]
    tmdb_token: Option<String>,
    #[clap(short, long, env)]
    mode: Option<String>,
    #[clap(long)]
    token_ttl_seconds: Option<i64>,
    #[clap(long, value_delimiter = ',')]
    sources: Option<Vec<MetadataSourceKind>>,
    #[clap(long)]
    sync_anime_calendar_interval_seconds: Option<u64>,
    #[clap(long)]
    check_missing_episodes_interval_seconds: Option<u64>,
    #[clap(long)]
    fetch_resources_interval_seconds: Option<u64>,
    #[clap(long)]
    search_resources_interval_seconds: Option<u64>,
    #[clap(long)]
    noop_download_driver: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SchedulerFileConfig {
    addr: Option<String>,
    db_path: Option<String>,
    key: Option<String>,
    tmdb_token: Option<String>,
    mode: Option<String>,
    token_ttl_seconds: Option<i64>,
    source: Option<MetadataSourceKind>,
    sources: Option<Vec<MetadataSourceKind>>,
    download: Option<DownloadFileConfig>,
    jobs: Option<SchedulerJobsFileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DownloadFileConfig {
    noop_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SchedulerJobsFileConfig {
    sync_anime_calendar: Option<JobFileConfig>,
    check_missing_episodes: Option<JobFileConfig>,
    fetch_resources: Option<JobFileConfig>,
    search_resources: Option<JobFileConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct JobFileConfig {
    enabled: Option<bool>,
    interval_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    pub addr: String,
    pub db_path: String,
    pub key: String,
    pub tmdb_token: String,
    pub mode: String,
    pub token_ttl_seconds: i64,
    pub sources: Vec<MetadataSourceKind>,
    pub download: DownloadConfig,
    pub jobs: SchedulerJobsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadConfig {
    pub noop_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerJobsConfig {
    pub sync_anime_calendar: ScheduledJobConfig,
    pub check_missing_episodes: ScheduledJobConfig,
    pub fetch_resources: ScheduledJobConfig,
    pub search_resources: ScheduledJobConfig,
}

impl SchedulerConfig {
    pub fn load() -> Result<Self> {
        let args = SchedulerArgs::parse();
        Self::load_from_args(args)
    }

    pub fn load_from_args(args: SchedulerArgs) -> Result<Self> {
        let file_config = Self::read_file_config(args.config.as_deref())?;
        Self::merge(args, file_config)
    }

    fn read_file_config(config_path: Option<&str>) -> Result<SchedulerFileConfig> {
        if let Some(config_path) = config_path {
            let config_content = fs::read_to_string(config_path)?;
            Ok(toml::from_str::<SchedulerFileConfig>(&config_content)?)
        } else {
            Ok(SchedulerFileConfig::default())
        }
    }

    fn merge(args: SchedulerArgs, file_config: SchedulerFileConfig) -> Result<Self> {
        let file_jobs = file_config.jobs.clone().unwrap_or_default();
        let mode = args
            .mode
            .or(file_config.mode)
            .unwrap_or_else(|| "info".to_string());
        let mode = if matches!(mode.as_str(), "debug" | "warn" | "info") {
            mode
        } else {
            "info".to_string()
        };

        Ok(Self {
            addr: args
                .addr
                .or(file_config.addr)
                .unwrap_or_else(|| "127.0.0.1:1234".to_string()),
            db_path: args
                .db_path
                .or(file_config.db_path)
                .ok_or_else(|| anyhow!("db_path is required"))?,
            key: args
                .key
                .or(file_config.key)
                .ok_or_else(|| anyhow!("key is required"))?,
            tmdb_token: args
                .tmdb_token
                .or(file_config.tmdb_token)
                .ok_or_else(|| anyhow!("tmdb_token is required"))?,
            mode,
            token_ttl_seconds: args
                .token_ttl_seconds
                .or(file_config.token_ttl_seconds)
                .unwrap_or(60 * 60 * 24 * 30),
            sources: normalize_sources(args.sources, file_config.sources, file_config.source),
            download: DownloadConfig {
                noop_enabled: args
                    .noop_download_driver
                    .or(file_config
                        .download
                        .and_then(|download| download.noop_enabled))
                    .unwrap_or(false),
            },
            jobs: SchedulerJobsConfig {
                sync_anime_calendar: build_scheduled_job_config(
                    args.sync_anime_calendar_interval_seconds,
                    file_jobs.sync_anime_calendar,
                    12 * 60 * 60,
                    true,
                )?,
                check_missing_episodes: build_scheduled_job_config(
                    args.check_missing_episodes_interval_seconds,
                    file_jobs.check_missing_episodes,
                    24 * 60 * 60,
                    false,
                )?,
                fetch_resources: build_scheduled_job_config(
                    args.fetch_resources_interval_seconds,
                    file_jobs.fetch_resources,
                    5 * 60,
                    false,
                )?,
                search_resources: build_scheduled_job_config(
                    args.search_resources_interval_seconds,
                    file_jobs.search_resources,
                    5 * 60,
                    false,
                )?,
            },
        })
    }
}

fn build_scheduled_job_config(
    cli_interval_seconds: Option<u64>,
    file_job_config: Option<JobFileConfig>,
    default_interval_seconds: u64,
    immediate: bool,
) -> Result<ScheduledJobConfig> {
    let enabled = file_job_config
        .as_ref()
        .and_then(|job| job.enabled)
        .unwrap_or(true);
    let interval_seconds = cli_interval_seconds
        .or(file_job_config.and_then(|job| job.interval_seconds))
        .unwrap_or(default_interval_seconds);

    StrictScheduledJobConfigFactory::build(enabled, interval_seconds, immediate)
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn loads_config_from_temp_file() {
        let db_file = NamedTempFile::new().expect("temp db file");
        let config_file = NamedTempFile::new().expect("temp config file");
        fs::write(
            config_file.path(),
            format!(
                r#"
db_path = "{}"
key = "file-key"
tmdb_token = "token-from-config"
addr = "127.0.0.1:4000"
mode = "warn"
token_ttl_seconds = 3600
source = "yuc"
[download]
noop_enabled = true
[jobs.sync_anime_calendar]
interval_seconds = 600
[jobs.check_missing_episodes]
enabled = false
interval_seconds = 86400
[jobs.fetch_resources]
interval_seconds = 300
[jobs.search_resources]
interval_seconds = 600
"#,
                path_to_string(db_file.path().to_path_buf())
            ),
        )
        .expect("write config file");

        let config = SchedulerConfig::load_from_args(SchedulerArgs {
            config: Some(path_to_string(config_file.path().to_path_buf())),
            ..SchedulerArgs::default()
        })
        .expect("load config");

        assert_eq!(
            config,
            SchedulerConfig {
                addr: "127.0.0.1:4000".to_string(),
                db_path: path_to_string(db_file.path().to_path_buf()),
                key: "file-key".to_string(),
                tmdb_token: "token-from-config".to_string(),
                mode: "warn".to_string(),
                token_ttl_seconds: 3600,
                sources: vec![MetadataSourceKind::Yuc],
                download: DownloadConfig { noop_enabled: true },
                jobs: SchedulerJobsConfig {
                    sync_anime_calendar: ScheduledJobConfig {
                        enabled: true,
                        interval: job::config::JobInterval { seconds: 600 },
                        immediate: true,
                    },
                    check_missing_episodes: ScheduledJobConfig {
                        enabled: false,
                        interval: job::config::JobInterval { seconds: 86400 },
                        immediate: false,
                    },
                    fetch_resources: ScheduledJobConfig {
                        enabled: true,
                        interval: job::config::JobInterval { seconds: 300 },
                        immediate: false,
                    },
                    search_resources: ScheduledJobConfig {
                        enabled: true,
                        interval: job::config::JobInterval { seconds: 600 },
                        immediate: false,
                    },
                },
            }
        );
    }

    #[test]
    fn cli_overrides_config_file() {
        let db_file_from_config = NamedTempFile::new().expect("temp config db file");
        let db_file_from_cli = NamedTempFile::new().expect("temp cli db file");
        let config_file = NamedTempFile::new().expect("temp config file");
        fs::write(
            config_file.path(),
            format!(
                r#"
db_path = "{}"
key = "file-key"
tmdb_token = "file-token"
addr = "127.0.0.1:4000"
mode = "info"
token_ttl_seconds = 3600
sources = ["bangumi"]
[jobs.sync_anime_calendar]
interval_seconds = 600
[jobs.check_missing_episodes]
enabled = false
interval_seconds = 86400
[jobs.fetch_resources]
interval_seconds = 300
[jobs.search_resources]
interval_seconds = 600
"#,
                path_to_string(db_file_from_config.path().to_path_buf())
            ),
        )
        .expect("write config file");

        SchedulerConfig::load_from_args(SchedulerArgs {
            config: Some(path_to_string(config_file.path().to_path_buf())),
            addr: Some("127.0.0.1:5000".to_string()),
            db_path: Some(path_to_string(db_file_from_cli.path().to_path_buf())),
            key: Some("cli-key".to_string()),
            tmdb_token: Some("cli-token".to_string()),
            mode: Some("debug".to_string()),
            token_ttl_seconds: Some(7200),
            sources: Some(vec![MetadataSourceKind::Yuc, MetadataSourceKind::Bangumi]),
            sync_anime_calendar_interval_seconds: Some(42),
            check_missing_episodes_interval_seconds: Some(12),
            fetch_resources_interval_seconds: Some(42),
            search_resources_interval_seconds: Some(12),
            noop_download_driver: None,
        })
        .expect_err("interval too small must fail");
    }

    #[test]
    fn cli_overrides_config_file_with_valid_job_intervals() {
        let db_file_from_config = NamedTempFile::new().expect("temp config db file");
        let db_file_from_cli = NamedTempFile::new().expect("temp cli db file");
        let config_file = NamedTempFile::new().expect("temp config file");
        fs::write(
            config_file.path(),
            format!(
                r#"
db_path = "{}"
key = "file-key"
tmdb_token = "file-token"
mode = "info"
sources = ["bangumi"]
[jobs.sync_anime_calendar]
interval_seconds = 600
[jobs.check_missing_episodes]
enabled = false
interval_seconds = 86400
[jobs.fetch_resources]
interval_seconds = 300
[jobs.search_resources]
interval_seconds = 600
"#,
                path_to_string(db_file_from_config.path().to_path_buf())
            ),
        )
        .expect("write config file");

        let config = SchedulerConfig::load_from_args(SchedulerArgs {
            config: Some(path_to_string(config_file.path().to_path_buf())),
            addr: Some("127.0.0.1:5000".to_string()),
            db_path: Some(path_to_string(db_file_from_cli.path().to_path_buf())),
            key: Some("cli-key".to_string()),
            tmdb_token: Some("cli-token".to_string()),
            mode: Some("debug".to_string()),
            token_ttl_seconds: Some(7200),
            sources: Some(vec![MetadataSourceKind::Yuc, MetadataSourceKind::Bangumi]),
            sync_anime_calendar_interval_seconds: Some(600),
            check_missing_episodes_interval_seconds: Some(300),
            fetch_resources_interval_seconds: Some(300),
            search_resources_interval_seconds: Some(600),
            noop_download_driver: Some(true),
        })
        .expect("load config");

        assert_eq!(config.addr, "127.0.0.1:5000");
        assert_eq!(
            config.db_path,
            path_to_string(db_file_from_cli.path().to_path_buf())
        );
        assert_eq!(config.key, "cli-key");
        assert_eq!(config.tmdb_token, "cli-token");
        assert_eq!(config.token_ttl_seconds, 7200);
        assert_eq!(
            config.sources,
            vec![MetadataSourceKind::Yuc, MetadataSourceKind::Bangumi]
        );
        assert!(config.download.noop_enabled);
        assert_eq!(
            config.jobs.sync_anime_calendar,
            ScheduledJobConfig {
                enabled: true,
                interval: job::config::JobInterval { seconds: 600 },
                immediate: true,
            }
        );
        assert_eq!(
            config.jobs.check_missing_episodes,
            ScheduledJobConfig {
                enabled: false,
                interval: job::config::JobInterval { seconds: 300 },
                immediate: false,
            }
        );
        assert_eq!(
            config.jobs.search_resources,
            ScheduledJobConfig {
                enabled: true,
                interval: job::config::JobInterval { seconds: 600 },
                immediate: false,
            }
        );
        assert_eq!(
            config.jobs.fetch_resources,
            ScheduledJobConfig {
                enabled: true,
                interval: job::config::JobInterval { seconds: 300 },
                immediate: false,
            }
        );
    }

    #[test]
    fn loads_legacy_single_source_field_for_compatibility() {
        let config_file = NamedTempFile::new().expect("temp config file");
        let db_file = NamedTempFile::new().expect("temp db file");
        fs::write(
            config_file.path(),
            format!(
                r#"
db_path = "{}"
key = "file-key"
tmdb_token = "token"
source = "yuc"
"#,
                path_to_string(db_file.path().to_path_buf())
            ),
        )
        .expect("write config file");

        let config = SchedulerConfig::load_from_args(SchedulerArgs {
            config: Some(path_to_string(config_file.path().to_path_buf())),
            ..SchedulerArgs::default()
        })
        .expect("load config");

        assert_eq!(config.sources, vec![MetadataSourceKind::Yuc]);
    }

    fn path_to_string(path: PathBuf) -> String {
        path.to_string_lossy().to_string()
    }
}
