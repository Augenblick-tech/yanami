use std::sync::Arc;

use anna::rss::rss::RssHttpClient;
use axum::{
    body::Body,
    extract::MatchedPath,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Extension, RequestPartsExt, Router,
};
use axum_extra::TypedHeader;
use headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, Validation};
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    common::{
        auth::{Claims, KEYS},
        result::JsonResult,
    },
    hander::{
        anime::{anime_records, animes},
        path::{get_path, set_path},
        rss::{del_rss, rss_list, set_rss},
        rule::{del_rule, rules, set_rule},
        user::{login, register, register_code, users},
    },
    openapi::ApiDoc,
    provider::db::db_provider::{
        AnimeProvider, DbProvider, DownloadPathProvider, RssProvider, RuleProvider, UserProvider,
    },
};

#[derive(Clone)]
pub struct Service {
    pub user_db: UserProvider,
    pub db: DbProvider,
    pub rss_db: RssProvider,
    pub rule_db: RuleProvider,
    pub anime_db: AnimeProvider,
    pub rss_http_client: Arc<RssHttpClient>,
    pub path: DownloadPathProvider,
}

impl Service {
    pub fn new(
        user_db: UserProvider,
        db: DbProvider,
        rss_db: RssProvider,
        rule_db: RuleProvider,
        anime_db: AnimeProvider,
        rss_http_client: Arc<RssHttpClient>,
        path: DownloadPathProvider,
    ) -> Self {
        Service {
            user_db,
            db,
            rss_db,
            rule_db,
            anime_db,
            rss_http_client,
            path,
        }
    }
}

pub fn route(service: Service) -> Router {
    let v1_auth = Router::new()
        .route("/register/code", get(register_code))
        .route("/users", get(users))
        .route("/rss", get(rss_list))
        .route("/rss", post(set_rss))
        .route("/rss", delete(del_rss))
        .route("/rule", post(set_rule))
        .route("/rules", get(rules))
        .route("/rule", delete(del_rule))
        .route("/path", post(set_path))
        .route("/path", get(get_path))
        .route("/animes", get(animes))
        .route("/anime/records", get(anime_records))
        .layer(Extension(service.clone()))
        .route_layer(middleware::from_fn(auth));

    let v1 = Router::new()
        .route("/login", post(login))
        .route("/ping", get(|| async { JsonResult::json("pong") }))
        .route("/register", post(register))
        .nest("/", v1_auth)
        .layer(Extension(service.clone()));

    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(Redoc::with_url("/redoc", ApiDoc::openapi()))
        .nest("/v1", v1)
        .route_layer(middleware::from_fn(log))
        .fallback(handler_404)
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "404 not found")
}

async fn log(request: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let path = if let Some(matched_path) = request.extensions().get::<MatchedPath>() {
        matched_path.as_str().to_owned()
    } else {
        request.uri().path().to_owned()
    };

    tracing::debug!(
        "method: {}, path: {}, body: {:?}",
        &request.method(),
        &path,
        &request.body(),
    );
    Ok(next.run(request).await)
}

async fn auth(request: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let (mut parts, body) = request.into_parts();

    let TypedHeader(Authorization(bearer)) = parts
        .extract::<TypedHeader<Authorization<Bearer>>>()
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let claims = match decode::<Claims>(
        bearer.token(),
        &KEYS.get().unwrap().decoding,
        &Validation::default(),
    ) {
        Ok(token_data) => token_data.claims,
        Err(err) => {
            tracing::debug!("decode err {}", err);
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let mut request = Request::from_parts(parts, body);
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}
