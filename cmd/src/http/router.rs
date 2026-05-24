use std::sync::Arc;

use axum::{
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use crate::http::{auth::require_auth, handler, openapi::ApiDoc, state::AppState, static_assets};

pub fn build_router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/users/me/password", put(handler::change_password))
        .route(
            "/users/me/download",
            get(handler::get_download_configuration),
        )
        .route(
            "/users/me/download/driver",
            put(handler::select_download_driver),
        )
        .route("/users/me/download/qbit", put(handler::save_qbit_profile))
        .route(
            "/animes",
            get(handler::list_animes).post(handler::create_anime),
        )
        .route("/animes/dashboard", get(handler::get_anime_dashboard))
        .route("/animes/latest", get(handler::list_latest_anime_releases))
        .route("/animes/preview", get(handler::preview_anime))
        .route(
            "/animes/:anime_id",
            get(handler::get_anime).post(handler::update_anime),
        )
        .route(
            "/animes/:anime_id/records",
            get(handler::list_anime_release_records),
        )
        .route(
            "/animes/:anime_id/metadata",
            get(handler::get_anime_metadata).put(handler::update_anime_metadata),
        )
        .route(
            "/animes/:anime_id/subscription",
            post(handler::subscribe).delete(handler::unsubscribe),
        )
        .route(
            "/animes/:anime_id/subscription/active",
            put(handler::set_subscription_active),
        )
        .route(
            "/space/rules",
            get(handler::get_rules).post(handler::create_rule),
        )
        .route(
            "/space/rules/:rule_id",
            put(handler::update_rule).delete(handler::delete_rule),
        )
        .route(
            "/space/feeds",
            get(handler::get_feeds).post(handler::create_feed),
        )
        .route(
            "/space/feeds/:feed_id",
            put(handler::update_feed).delete(handler::delete_feed),
        )
        .route(
            "/space/auto-subscribe",
            get(handler::get_auto_subscribe).put(handler::set_auto_subscribe),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let api = Router::new()
        .route("/ping", get(handler::ping))
        .route("/auth/login", post(handler::login))
        .nest("/", protected)
        .fallback(api_not_found);

    Router::new()
        .nest("/api/v1", api)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .route("/", get(static_assets::index))
        .fallback(static_assets::serve)
        .with_state(state)
}

async fn api_not_found() -> impl IntoResponse {
    StatusCode::NOT_FOUND
}
