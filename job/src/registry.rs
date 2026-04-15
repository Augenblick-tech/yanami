use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JobRegistryError {
    #[error("duplicate job name: {0}")]
    DuplicateJobName(String),
}

pub fn ensure_unique_job_names<I>(job_names: I) -> Result<(), JobRegistryError>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut seen = HashSet::new();
    for job_name in job_names {
        let job_name = job_name.as_ref().to_string();
        if !seen.insert(job_name.clone()) {
            return Err(JobRegistryError::DuplicateJobName(job_name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_job_names() {
        let error = ensure_unique_job_names([
            "sync_anime_calendar",
            "check_missing_episodes",
            "sync_anime_calendar",
        ])
        .expect_err("duplicate names must fail");

        assert_eq!(
            error,
            JobRegistryError::DuplicateJobName("sync_anime_calendar".to_string())
        );
    }

    #[test]
    fn accepts_unique_job_names() {
        let result = ensure_unique_job_names(["sync_anime_calendar", "check_missing_episodes"]);

        assert!(result.is_ok());
    }
}
