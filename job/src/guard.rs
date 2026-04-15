use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub struct InMemoryJobGuard {
    running_jobs: Arc<Mutex<HashSet<String>>>,
}

impl InMemoryJobGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_acquire(&self, job_name: &str) -> Option<JobPermit> {
        let mut running_jobs = self
            .running_jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if running_jobs.contains(job_name) {
            return None;
        }
        running_jobs.insert(job_name.to_string());
        Some(JobPermit {
            running_jobs: Arc::clone(&self.running_jobs),
            job_name: job_name.to_string(),
        })
    }
}

pub struct JobPermit {
    running_jobs: Arc<Mutex<HashSet<String>>>,
    job_name: String,
}

impl Drop for JobPermit {
    fn drop(&mut self) {
        let mut running_jobs = self
            .running_jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        running_jobs.remove(&self.job_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_rejects_duplicate_running_job() {
        let guard = InMemoryJobGuard::new();

        let permit = guard.try_acquire("sync_anime_calendar");
        let duplicate = guard.try_acquire("sync_anime_calendar");

        assert!(permit.is_some());
        assert!(duplicate.is_none());
    }

    #[test]
    fn guard_releases_job_after_permit_drop() {
        let guard = InMemoryJobGuard::new();

        {
            let permit = guard.try_acquire("sync_anime_calendar");
            assert!(permit.is_some());
        }

        let reacquired = guard.try_acquire("sync_anime_calendar");
        assert!(reacquired.is_some());
    }
}
