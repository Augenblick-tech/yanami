use chrono::{Datelike, NaiveDate};

use crate::gateway::{TmdbSearchResultItem, TmdbSeriesDetails};
use crate::source::AnimeMetadataSeed;

pub fn select_tmdb_result<'a>(
    item: &AnimeMetadataSeed,
    results: &'a [TmdbSearchResultItem],
) -> Option<&'a TmdbSearchResultItem> {
    if let Ok(calendar_date) = NaiveDate::parse_from_str(&item.air_date, "%Y-%m-%d") {
        for result in results {
            if let Some(first_air_date) = &result.first_air_date {
                if let Ok(first_air_date) = NaiveDate::parse_from_str(first_air_date, "%Y-%m-%d") {
                    if first_air_date.year() == calendar_date.year()
                        && first_air_date.month() == calendar_date.month()
                    {
                        return Some(result);
                    }
                }
            }
        }
    }
    results.first()
}

pub fn select_air_date(item: &AnimeMetadataSeed, series: &TmdbSeriesDetails) -> String {
    if let Some(first_air_date) = &series.first_air_date {
        if let Ok(series_date) = NaiveDate::parse_from_str(first_air_date, "%Y-%m-%d") {
            if let Ok(item_date) = NaiveDate::parse_from_str(&item.air_date, "%Y-%m-%d") {
                if item_date < series_date {
                    return item.air_date.clone();
                }
            } else {
                return item.air_date.clone();
            }
        } else {
            return item.air_date.clone();
        }
        return first_air_date.clone();
    }
    item.air_date.clone()
}
