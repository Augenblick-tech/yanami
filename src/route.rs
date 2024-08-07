use std::sync::Arc;

use axum::{
    body::Body,
    extract::MatchedPath,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Extension, RequestPartsExt, Router,
};
use axum_extra::TypedHeader;
use headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, Validation};

use crate::{
    common::auth::{Claims, KEYS},
    provider::provider::Provider,
};

#[derive(Clone)]
pub struct ServiceRegister {
    pub provider: Arc<dyn Provider + Send + Sync>,
}

impl ServiceRegister {
    pub fn new(provider: Arc<dyn Provider + Send + Sync>) -> Self {
        ServiceRegister { provider }
    }
}

pub fn route(service_register: ServiceRegister) -> Router {
    let v1_auth = Router::new()
        // .route("/login", post(login))
        // .route("/info", get(info))
        .route("/hello", get(|| async { "hello axum" }))
        .layer(Extension(service_register))
        .route_layer(middleware::from_fn(auth));

    // let v1 = Router::new()
    //     .route("/login", post(login))
    //     .nest("/", v1_auth)
    //     .layer(Extension(service_register.user_service.clone()));

    Router::new()
        .nest("/api/v1", v1_auth)
        // .nest("/api/v2", v2)
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
