use utoipa::OpenApi;

use crate::http::{error::ErrorResponse, handler, model::*};

#[derive(OpenApi)]
#[openapi(
    paths(
        handler::ping,
        handler::login,
        handler::change_password,
        handler::get_download_configuration,
        handler::select_download_driver,
        handler::save_qbit_profile,
        handler::list_animes,
        handler::get_anime,
        handler::get_anime_dashboard,
        handler::list_latest_anime_releases,
        handler::list_anime_release_records,
        handler::update_anime,
        handler::preview_anime,
        handler::create_anime,
        handler::get_anime_metadata,
        handler::update_anime_metadata,
        handler::subscribe,
        handler::unsubscribe,
        handler::set_subscription_active,
        handler::get_rules,
        handler::create_rule,
        handler::delete_rule,
        handler::get_feeds,
        handler::create_feed,
        handler::update_feed,
        handler::delete_feed,
        handler::get_auto_subscribe,
        handler::set_auto_subscribe,
    ),
    components(
        schemas(
            SetAutoSubscribeRequest,
            AutoSubscribeResponse,
            LoginRequest,
            ChangePasswordRequest,
            CreateAnimeRequest,
            UpdateAnimeMetadataRequest,
            SetSubscriptionRequest,
            PatchAnimeRequest,
            SaveFeedSourceRequest,
            MatchingRuleRequest,
            SaveQbitProfileRequest,
            SelectDownloadDriverRequest,
            AnimeIdParam,
            AnimeLanguageQuery,
            BgmIdQuery,
            LatestAnimeParam,
            AnimeQuery,
            AnimeDashboardResponse,
            AnimeDashboardSearchResponse,
            AnimeDashboardQuarterResponse,
            AnimeDashboardStatsResponse,
            AnimeReleaseRecordsResponse,
            AnimeReleaseRecordResponse,
            LoginResponse,
            ChangePasswordResponse,
            UserView,
            FeedsResponse,
            FeedSourceView,
            DeleteFeedSourceResponse,
            RulesResponse,
            MatchingRuleView,
            DeleteMatchingRuleResponse,
            AnimeViewResponse,
            LatestAnimeViewResponse,
            ErrorResponse,
            DownloadConfigurationResponse,
            QbitProfileResponse,
            DriverResponse,
            PingResponse,
            UsizeApiResponse,
        )
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}
