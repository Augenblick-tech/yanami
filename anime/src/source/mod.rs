mod contracts;
mod metadata_builder;
#[cfg(test)]
mod tests;
mod tmdb_matcher;

pub use contracts::{
    AnimeMetadataSeed, AnimeSource, AnimeSourceFactory, LoadTmdbAlternativeTitles,
    LoadTmdbSeriesDetails, SearchTmdbTv, SingleAnimeSource,
};
pub use metadata_builder::build_anime_metadata;
pub use tmdb_matcher::{select_air_date, select_tmdb_result};
