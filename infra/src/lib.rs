//! Anime context infrastructure implementations.

pub mod anime_source;
pub mod bangumi;
pub mod db;
pub mod noop_download;
pub mod qbit;
pub mod rss;
pub mod rule_runtime;
pub mod secret;
pub mod seed_source;
pub mod tmdb;
pub mod user;
pub mod yuc;

#[cfg(test)]
mod seed_source_tests;
