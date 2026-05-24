use std::sync::Arc;

use anime::source::SingleAnimeSource;
use domain::space::SpaceId;
use domain::user::UserId;

use crate::http::auth::JwtDecoder;
use service::{
    anime::service::AnimeService, download::service::DownloadService, feed::service::FeedService,
    rule::service::RuleService, space::service::SpaceService,
    subscription::service::SubscriptionService, user::service::UserService,
};

/// HTTP 服务共享状态。
pub struct AppState {
    /// JWT 解码器。
    pub auth: JwtDecoder,
    pub space_service: Arc<SpaceService>,
    pub user_service: Arc<UserService>,
    pub rule_service: Arc<RuleService>,
    pub feed_service: Arc<FeedService>,
    pub download_service: Arc<DownloadService>,
    pub anime_service: Arc<AnimeService>,
    pub subscription_service: Arc<SubscriptionService>,
    pub single_anime_source: Arc<dyn SingleAnimeSource>,
    pub admin_user_id: UserId,
    pub admin_space_id: SpaceId,
}
