mod check_missing_episodes;
mod fetch_resources;
mod match_resources;
mod sync_anime_calendar;

use async_trait::async_trait;

use crate::shared::error::ApplicationError;

pub use check_missing_episodes::CheckMissingEpisodesJob;
pub use fetch_resources::FetchResourcesJob;
pub use match_resources::MatchResourcesJob;
pub use sync_anime_calendar::SyncAnimeCalendarJob;

#[async_trait]
pub trait Job: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self) -> Result<(), ApplicationError>;
}
