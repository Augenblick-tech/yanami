mod bootstrap;
mod config;
mod http;
mod matched_resource_action;
mod metadata;
mod local_match_runner;
mod pool_consumer;

use anyhow::Result;

use crate::{bootstrap::run, config::SchedulerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    start_with(SchedulerConfig::load, run).await
}

async fn start_with<LoadConfig, RunApp, RunFuture>(
    load_config: LoadConfig,
    run_app: RunApp,
) -> Result<()>
where
    LoadConfig: FnOnce() -> Result<SchedulerConfig>,
    RunApp: FnOnce(SchedulerConfig) -> RunFuture,
    RunFuture: std::future::Future<Output = Result<()>>,
{
    let config = load_config()?;
    run_app(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> SchedulerConfig {
        SchedulerConfig {
            addr: "127.0.0.1:1234".to_string(),
            db_path: "sqlite::memory:".to_string(),
            key: "test-key".to_string(),
            tmdb_token: "test-token".to_string(),
            mode: "info".to_string(),
            token_ttl_seconds: 3600,
            sources: vec![],
            download: crate::config::DownloadConfig {
                noop_enabled: false,
            },
            jobs: crate::config::SchedulerJobsConfig {
                sync_anime_calendar: job::config::ScheduledJobConfig {
                    enabled: false,
                    interval: job::config::JobInterval { seconds: 600 },
                    immediate: true,
                },
                check_missing_episodes: job::config::ScheduledJobConfig {
                    enabled: false,
                    interval: job::config::JobInterval { seconds: 300 },
                    immediate: false,
                },
                fetch_resources: job::config::ScheduledJobConfig {
                    enabled: false,
                    interval: job::config::JobInterval { seconds: 300 },
                    immediate: false,
                },
                search_resources: job::config::ScheduledJobConfig {
                    enabled: false,
                    interval: job::config::JobInterval { seconds: 300 },
                    immediate: false,
                },
            },
            log_file: None,
        }
    }

    #[tokio::test]
    async fn start_with_loads_config_and_runs_app() {
        let config = sample_config();
        let expected_addr = config.addr.clone();
        let output = start_with(
            || Ok(config.clone()),
            |loaded| async move {
                assert_eq!(loaded.addr, expected_addr);
                Ok(())
            },
        )
        .await;

        assert!(output.is_ok());
    }

    #[tokio::test]
    async fn start_with_propagates_load_error() {
        let error = start_with(
            || Err(anyhow::anyhow!("load failed")),
            |_loaded| async move { Ok(()) },
        )
        .await
        .expect_err("load must fail");

        assert_eq!(error.to_string(), "load failed");
    }
}
