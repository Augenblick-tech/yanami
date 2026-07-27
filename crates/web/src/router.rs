use std::sync::Arc;

use axum::{
    Router,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
};
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    app_ctx::AppContext,
    handler::{anime, downloader, feed, rule, stat, subscription, user},
    middleware::auth::{require_admin, require_auth},
};

pub fn route(ctx: Arc<AppContext>) -> Router {
    let admin = Router::new()
        .route("/feed", post(feed::add))
        .route("/feed/{feed_id}", delete(feed::delete).put(feed::edit))
        .route("/system/log-level", put(stat::set_log_level))
        .layer(middleware::from_fn(require_admin));

    let auth = Router::new()
        .route("/anime", post(anime::list))
        .route("/anime/search", get(anime::search))
        .route("/anime/bgm/{bgm_id}", get(anime::bgm_info))
        .route("/anime/create", post(anime::create))
        .route("/anime/{anime_id}", put(anime::edit))
        .route("/feed", get(feed::list))
        .route("/rule", post(rule::add).get(rule::list))
        .route("/rule/{rule_id}", put(rule::edit).delete(rule::delete))
        .route("/subscription", post(subscription::add))
        .route("/subscription/recent", get(subscription::recent_episodes))
        .route("/subscription/{id}", delete(subscription::delete))
        .route("/subscription/{id}/episode", get(subscription::list_eps))
        .route(
            "/subscription/{id}/search_status",
            post(subscription::set_search_status),
        )
        .route(
            "/subscription/{id}/bind_rule",
            post(subscription::bind_rule),
        )
        .route("/subscription/{id}/eps", put(subscription::reset_all_eps))
        .route(
            "/subscription/{id}/eps/{ep_id}",
            put(subscription::update_ep_status),
        )
        .route("/stat", get(stat::get_system_stat))
        .route(
            "/user/download/config",
            get(user::list_download_config).post(user::save_download_config),
        )
        .route(
            "/user/download/config/active",
            put(user::switch_active_download_config),
        )
        .route(
            "/user/download/config/{name}",
            delete(user::delete_download_config),
        )
        .route("/user/password", post(user::change_password))
        .route(
            "/user/auto_sub",
            post(user::toggle_auto_sub).get(user::get_auto_sub),
        )
        .route("/downloader/tasks", get(downloader::list_tasks))
        .route(
            "/downloader/tasks/{hash}",
            put(downloader::update_task_state).delete(downloader::delete_task),
        )
        .merge(admin)
        .layer(middleware::from_fn_with_state(ctx.clone(), require_auth));

    let api = Router::new()
        .route("/user/login", post(user::login))
        .merge(auth)
        .fallback(api_not_found);

    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .nest("/api/v1", api)
        .fallback(crate::handler::static_files::static_handler)
        .with_state(ctx)
}

async fn api_not_found() -> impl IntoResponse {
    StatusCode::NOT_FOUND
}

#[derive(OpenApi)]
#[openapi(
    paths(
        anime::list,
        anime::search,
        anime::bgm_info,
        anime::create,
        anime::edit,
        feed::add,
        feed::list,
        feed::delete,
        feed::edit,
        rule::add,
        rule::list,
        rule::edit,
        rule::delete,
        subscription::add,
        subscription::delete,
        subscription::list_eps,
        subscription::recent_episodes,
        subscription::set_search_status,
        subscription::bind_rule,
        subscription::reset_all_eps,
        subscription::update_ep_status,
        user::list_download_config,
        user::save_download_config,
        user::delete_download_config,
        user::switch_active_download_config,
        user::change_password,
        user::toggle_auto_sub,
        user::get_auto_sub,
        user::login,
        stat::get_system_stat,
        stat::set_log_level,
        downloader::list_tasks,
        downloader::update_task_state,
        downloader::delete_task,
    ),
    components(
        schemas(
            crate::model::LogLevelRequest,
            crate::model::LoginRequest,
            crate::model::ChangePasswordRequest,
            crate::model::SwitchActiveDownloaderRequest,
            crate::model::AutoSubRequest,
            crate::model::AutoSubResponse,
            crate::model::CreateSubscriptionRequest,
            crate::model::RecentEpisodeResponse,
            crate::model::RecentEpisodeQuery,
            crate::model::SearchStatusRequest,
            crate::model::BindRuleRequest,
            crate::model::EditAnimeRequest,
            crate::model::PageAnimeRequest,
            crate::error::ErrorResponse,
            crate::model::AnimeResponse,
            crate::model::AnimeSubInfo,
            crate::model::LoginResponse,
            crate::model::SystemStatResponse,
            crate::model::QuarterStat,
            crate::model::BackoffFeed,
            crate::model::FeedItemRequest,
            crate::model::FeedItem,
            crate::model::QbitSettings,
            crate::model::DefaultDownloaderSettings,
            crate::model::DownloaderSettings,
            crate::model::DownloadTaskResponse,
            crate::model::DownloadTaskActionRequest,

            crate::model::RuleCreateRequest,
            crate::model::RuleUpdateOrderRequest,
            crate::model::RuleItem,
            crate::model::EpisodeItem,
            crate::model::SearchAnimeQuery,
            crate::model::CreateAnimeRequest,
            crate::model::SearchAnimeItem,
            crate::model::AnimeAirWeekdayItem,
            crate::model::AnimeLangTargetItem,
            crate::model::AnimeIdTypeItem,
            crate::model::AnimeSourceTargetItem,
            crate::model::AnimeTitleItem,
            crate::model::AnimeExItem,
            crate::model::AnimeEpisodeItem,
            crate::model::AnimeSeasonItem,
            crate::model::AnimeMetadataItem,
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
                "jwt",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}
