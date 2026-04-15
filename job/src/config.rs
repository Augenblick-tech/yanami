use domain::shared::error::DomainError;

const MIN_JOB_INTERVAL_SECONDS: u64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobInterval {
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledJobConfig {
    pub enabled: bool,
    pub interval: JobInterval,
    pub immediate: bool,
}

pub struct StrictScheduledJobConfigFactory;

impl StrictScheduledJobConfigFactory {
    pub fn build(
        enabled: bool,
        interval_seconds: u64,
        immediate: bool,
    ) -> Result<ScheduledJobConfig, DomainError> {
        if interval_seconds < MIN_JOB_INTERVAL_SECONDS {
            return Err(DomainError::InvariantViolation(
                "job interval must be at least 5 minutes",
            ));
        }

        Ok(ScheduledJobConfig {
            enabled,
            interval: JobInterval {
                seconds: interval_seconds,
            },
            immediate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_too_small_job_interval() {
        let error = StrictScheduledJobConfigFactory::build(true, 299, false)
            .expect_err("interval must be rejected");

        assert_eq!(
            error.to_string(),
            "domain invariant violation: job interval must be at least 5 minutes"
        );
    }

    #[test]
    fn accepts_valid_job_interval() {
        let config =
            StrictScheduledJobConfigFactory::build(true, 300, false).expect("valid config");

        assert_eq!(config.interval.seconds, 300);
    }
}
