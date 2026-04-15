use domain::{
    shared::error::DomainError,
    subscription::MissingEpisodeAssessment,
};

use crate::episode_extractor::extract_episode_numbers;

pub struct MissingEpisodeChecker;

impl MissingEpisodeChecker {
    pub fn assess_missing_episodes(
        &self,
        release_titles: &[String],
    ) -> Result<Option<MissingEpisodeAssessment>, DomainError> {
        assess_missing_episode_gap(release_titles)
    }
}

pub(crate) fn assess_missing_episode_gap(
    release_titles: &[String],
) -> Result<Option<MissingEpisodeAssessment>, DomainError> {
    let episodes = extract_episode_numbers(release_titles);
    Ok(check_missing_episode_status(&episodes))
}

fn check_missing_episode_status(episodes: &[i64]) -> Option<MissingEpisodeAssessment> {
    if episodes.len() <= 2 {
        return None;
    }

    let min_episode = *episodes.first()?;
    let max_episode = *episodes.last()?;
    let actual_count = episodes.len() as i64;
    let expected_count = max_episode - min_episode + 1;
    let missing_count = expected_count - actual_count;

    Some(MissingEpisodeAssessment {
        missing_count,
        actual_count,
        min_episode,
        max_episode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn reports_missing_episodes_for_small_gaps() {
        let assessment = assess_missing_episode_gap(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 04 [1080P]"),
        ])
        .expect("assessment succeeds")
        .expect("assessment exists");

        assert_eq!(assessment.missing_count, 1);
    }

    #[test]
    fn consecutive_episodes_no_missing() {
        let assessment = assess_missing_episode_gap(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 03 [1080P]"),
        ])
        .expect("assessment succeeds")
        .expect("assessment exists");

        assert_eq!(assessment.missing_count, 0);
        assert_eq!(assessment.actual_count, 3);
        assert_eq!(assessment.min_episode, 1);
        assert_eq!(assessment.max_episode, 3);
    }

    #[test]
    fn second_season_start_from_13() {
        let assessment = assess_missing_episode_gap(&[
            title("[ANi] Show S2 - 13 [1080P]"),
            title("[ANi] Show S2 - 14 [1080P]"),
            title("[ANi] Show S2 - 16 [1080P]"),
        ])
        .expect("assessment succeeds")
        .expect("assessment exists");

        assert_eq!(assessment.missing_count, 1);
        assert_eq!(assessment.min_episode, 13);
        assert_eq!(assessment.max_episode, 16);
    }

    #[test]
    fn gap_of_5_is_detected() {
        let assessment = assess_missing_episode_gap(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 08 [1080P]"),
        ])
        .expect("assessment succeeds")
        .expect("assessment exists");

        assert_eq!(assessment.missing_count, 5);
        assert_eq!(assessment.actual_count, 3);
    }

    #[test]
    fn large_gap_detected() {
        let assessment = assess_missing_episode_gap(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 03 [1080P]"),
            title("[ANi] Show - 1080 [1080P]"),
        ])
        .expect("assessment succeeds")
        .expect("assessment exists");

        assert_eq!(assessment.missing_count, 1076);
        assert_eq!(assessment.actual_count, 4);
    }

    #[test]
    fn fewer_than_3_episodes_returns_none() {
        let single = assess_missing_episode_gap(&[title("[ANi] Show - 01 [1080P]")])
            .expect("assessment succeeds");
        assert!(single.is_none());

        let two = assess_missing_episode_gap(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
        ])
        .expect("assessment succeeds");
        assert!(two.is_none());
    }
}
