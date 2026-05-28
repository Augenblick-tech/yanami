//! Subscription context domain implementations.

pub mod action;
pub mod contracts;
pub mod entity;
pub mod episode_extractor;
pub mod keywords;
pub mod missing_episodes;
pub mod save_path;
pub mod search_pool;
pub mod subscription_animes;

pub mod shared {
    pub mod error;
}

pub use entity::SubscriptionAnimeEntity;
pub use subscription_animes::{SubscriptionAnimeListQuery, SubscriptionAnimes, SubscriptionCaps};
