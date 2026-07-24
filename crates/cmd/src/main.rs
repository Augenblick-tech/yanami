use mimalloc::MiMalloc;
use web::app_ctx::{AppContext, AuthConfig};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let (filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    init(reload_handle).await;
}

async fn init(reload_handle: tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>) {
    let config = cmd::config::AppConfig::load().expect("failed to load configuration");

    let reloader: std::sync::Arc<dyn Fn(String) -> Result<(), String> + Send + Sync> = std::sync::Arc::new(move |filter: String| {
        reload_handle.reload(&filter).map_err(|e| e.to_string())
    });

    let ctx = AppContext::new(
        &config.database.path,
        AuthConfig {
            token: config.auth.jwt_secret.clone(),
            expire: std::time::Duration::from_secs(config.auth.jwt_expire_seconds),
        },
        config.external.tmdb_token.clone(),
        reloader,
    )
    .await;
    let ctx = std::sync::Arc::new(ctx);

    ctx.init_database().await;

    let scheduler = cmd::task::builder::setup(
        ctx.roots.users.clone(),
        ctx.roots.animes.clone(),
        ctx.roots.anime_source.clone(),
        ctx.roots.sub_animes.clone(),
        ctx.roots.resources.clone(),
        ctx.roots.feeds.clone(),
        ctx.roots.search_mandates.clone(),
    )
    .await
    .unwrap();
    scheduler.start();

    let app = web::router::route(ctx.clone());
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind to address");
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app)
        .await
        .expect("failed to start web server");
}
