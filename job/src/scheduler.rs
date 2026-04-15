use std::sync::Arc;

use service::{job::Job, shared::error::ApplicationError};
use tokio::task::JoinHandle;
use tracing::{debug, error};

use crate::{config::ScheduledJobConfig, guard::InMemoryJobGuard};

#[derive(Clone, Default)]
pub struct TokioScheduler {
    guard: InMemoryJobGuard,
}

impl TokioScheduler {
    pub fn new(guard: InMemoryJobGuard) -> Self {
        Self { guard }
    }

    pub fn spawn_scheduled<J>(
        &self,
        job: Arc<J>,
        schedule: ScheduledJobConfig,
    ) -> Option<JoinHandle<()>>
    where
        J: Job + 'static,
    {
        if !schedule.enabled {
            debug!("skip disabled job");
            return None;
        }

        let guard = self.guard.clone();
        let interval_secs = schedule.interval.seconds;
        let immediate = schedule.immediate;
        Some(tokio::spawn(async move {
            if !immediate {
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
            loop {
                let job_name = job.name();
                let permit = guard.try_acquire(job_name);
                if let Some(permit) = permit {
                    debug!("job started, job_name={}", job_name);
                    if let Err(error) = job.run().await {
                        log_job_error(job_name, &error);
                    }
                    debug!("job finished, job_name={}", job_name);
                    drop(permit);
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
        }))
    }

    #[cfg(test)]
    fn spawn_interval_for_test<J>(
        &self,
        job: Arc<J>,
        interval: std::time::Duration,
    ) -> JoinHandle<()>
    where
        J: Job + 'static,
    {
        let guard = self.guard.clone();
        tokio::spawn(async move {
            loop {
                let job_name = job.name();
                let Some(permit) = guard.try_acquire(job_name) else {
                    continue;
                };
                if let Err(error) = job.run().await {
                    log_job_error(job_name, &error);
                }
                drop(permit);
                tokio::time::sleep(interval).await;
            }
        })
    }
}

fn log_job_error(job_name: &str, error: &ApplicationError) {
    error!(
        error = ?error,
        "job run failed, job_name={}, error={}",
        job_name,
        error
    );
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::{sync::Notify, time};

    use service::{job::Job, shared::error::ApplicationError};

    use super::*;

    struct SlowJob {
        started: Arc<AtomicUsize>,
        finished: Arc<AtomicUsize>,
        notify: Arc<Notify>,
    }

    #[async_trait]
    impl Job for SlowJob {
        fn name(&self) -> &'static str {
            "slow_job"
        }

        async fn run(&self) -> Result<(), ApplicationError> {
            self.started.fetch_add(1, Ordering::SeqCst);
            self.notify.notified().await;
            self.finished.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn scheduler_skips_overlapping_job_runs() {
        let started = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        let notify = Arc::new(Notify::new());
        let scheduler = TokioScheduler::new(InMemoryJobGuard::new());
        let job = Arc::new(SlowJob {
            started: Arc::clone(&started),
            finished: Arc::clone(&finished),
            notify: Arc::clone(&notify),
        });

        let handle = scheduler.spawn_interval_for_test(job, Duration::from_millis(20));

        time::sleep(Duration::from_millis(80)).await;
        assert_eq!(started.load(Ordering::SeqCst), 1);

        notify.notify_one();
        time::sleep(Duration::from_millis(40)).await;
        assert_eq!(finished.load(Ordering::SeqCst), 1);

        handle.abort();
    }

    struct ErrorJob {
        runs: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Job for ErrorJob {
        fn name(&self) -> &'static str {
            "error_job"
        }

        async fn run(&self) -> Result<(), ApplicationError> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            Err(ApplicationError::Domain(
                domain::shared::error::DomainError::InvariantViolation("boom"),
            ))
        }
    }

    #[test]
    fn scheduler_returns_none_for_disabled_jobs() {
        let scheduler = TokioScheduler::new(InMemoryJobGuard::new());
        let job = Arc::new(ErrorJob {
            runs: Arc::new(AtomicUsize::new(0)),
        });

        let handle = scheduler.spawn_scheduled(
            job,
            ScheduledJobConfig {
                enabled: false,
                interval: crate::config::JobInterval { seconds: 300 },
                immediate: false,
            },
        );

        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn scheduler_keeps_ticking_after_job_errors() {
        let runs = Arc::new(AtomicUsize::new(0));
        let scheduler = TokioScheduler::new(InMemoryJobGuard::new());
        let handle = scheduler.spawn_interval_for_test(
            Arc::new(ErrorJob {
                runs: Arc::clone(&runs),
            }),
            Duration::from_millis(20),
        );

        time::sleep(Duration::from_millis(70)).await;
        handle.abort();

        assert!(runs.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn scheduler_spawns_enabled_job() {
        let runs = Arc::new(AtomicUsize::new(0));
        let scheduler = TokioScheduler::new(InMemoryJobGuard::new());
        let handle = scheduler.spawn_scheduled(
            Arc::new(ErrorJob {
                runs: Arc::clone(&runs),
            }),
            ScheduledJobConfig {
                enabled: true,
                interval: crate::config::JobInterval { seconds: 1 },
                immediate: false,
            },
        );

        time::sleep(Duration::from_millis(10)).await;
        handle.expect("handle").abort();
    }
}
