use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

static EPISODE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(\.\d+)?").expect("episode regex"));

pub fn extract_episode_numbers(titles: &[String]) -> Vec<i64> {
    let parsed_titles = titles
        .iter()
        .map(|title| {
            EPISODE_PATTERN
                .captures_iter(title)
                .filter_map(|capture| capture[0].parse::<f64>().ok())
                .filter(|number| number.eq(&number.trunc()))
                .map(|number| number as i64)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    if parsed_titles.len() <= 2 {
        return parsed_titles
            .iter()
            .filter_map(|numbers| numbers.first().copied())
            .filter(|number| *number > 0)
            .collect();
    }

    let mut resolved = Vec::new();
    let Some(min_width) = parsed_titles.iter().map(Vec::len).min() else {
        return Vec::new();
    };
    for index in 0..min_width {
        let column = parsed_titles
            .iter()
            .filter_map(|numbers| numbers.get(index).copied())
            .collect::<Vec<_>>();

        let mut counts = HashMap::new();
        for number in &column {
            *counts.entry(*number).or_insert(0usize) += 1;
        }
        if counts.values().any(|count| *count > 2) {
            continue;
        }

        resolved = column;
        break;
    }

    resolved.sort_unstable();
    resolved.dedup();
    resolved.into_iter().filter(|number| *number > 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn extracts_single_episode_number_from_title() {
        let numbers = extract_episode_numbers(&[title("[ANi] Show - 01 [1080P]")]);
        assert_eq!(numbers, vec![1]);
    }

    #[test]
    fn extracts_multiple_episodes_from_titles() {
        let numbers = extract_episode_numbers(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 04 [1080P]"),
        ]);
        assert_eq!(numbers, vec![1, 2, 4]);
    }

    #[test]
    fn two_titles_same_episode_keeps_both() {
        let numbers = extract_episode_numbers(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 01 [1080P]"),
        ]);
        assert_eq!(numbers, vec![1, 1]);
    }

    #[test]
    fn filters_out_resolution_numbers() {
        let numbers = extract_episode_numbers(&[
            title("[ANi] Show - 01 [1080P]"),
            title("[ANi] Show - 02 [1080P]"),
            title("[ANi] Show - 03 [1080P]"),
            title("[ANi] Show - 04 [1080P]"),
        ]);
        assert_eq!(numbers, vec![1, 2, 3, 4]);
    }

    #[test]
    fn handles_single_title() {
        let numbers = extract_episode_numbers(&[title("Show - 05")]);
        assert_eq!(numbers, vec![5]);
    }

    #[test]
    fn empty_input_returns_empty() {
        let numbers = extract_episode_numbers(&[]);
        assert!(numbers.is_empty());
    }
}
