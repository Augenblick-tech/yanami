use std::sync::Arc;

use axum::{
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    app_ctx::AppContext,
    handler::{anime, feed, rule, stat, subscription, user},
    middleware::auth::{require_admin, require_auth},
};

pub fn route(ctx: Arc<AppContext>) -> Router {
    let admin = Router::new()
        .route("/feed", post(feed::add))
        .route("/feed/{feed_id}", delete(feed::delete).put(feed::edit))
        .layer(middleware::from_fn(require_admin));

    let auth = Router::new()
        .route("/anime", post(anime::list))
        .route("/anime/search", get(anime::search))
        .route("/anime/bgm/{bgm_id}", get(anime::bgm_info))
        .route("/anime/create", post(anime::create))
        .route("/feed", get(feed::list))
        .route("/rule", post(rule::add).get(rule::list))
        .route("/rule/{rule_id}", put(rule::edit).delete(rule::delete))
        .route("/subscription", post(subscription::add))
        .route("/subscription/{id}", delete(subscription::delete))
        .route("/subscription/{id}/episode", get(subscription::list_eps))
        .route("/stat", get(stat::get_system_stat))
        .route(
            "/user/download/config",
            get(user::list_download_config).post(user::save_download_config),
        )
        .route(
            "/user/download/config/{name}",
            delete(user::delete_download_config),
        )
        .route("/user/password", post(user::change_password))
        .route("/user/auto_sub", post(user::toggle_auto_sub).get(user::get_auto_sub))
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
        user::list_download_config,
        user::save_download_config,
        user::delete_download_config,
        user::change_password,
        user::toggle_auto_sub,
        user::get_auto_sub,
        user::login,
        stat::get_system_stat,
    ),
    components(
        schemas(
            crate::model::LoginRequest,
            crate::model::ChangePasswordRequest,
            crate::model::AutoSubRequest,
            crate::model::AutoSubResponse,
            crate::model::CreateSubscriptionRequest,
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
            crate::model::DownloaderSettings,
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
