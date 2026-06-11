use mimalloc::MiMalloc;
use web::app_ctx::{AppContext, AuthConfig};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();
    init().await;
}

async fn init() {
    let config = cmd::config::AppConfig::load().expect("failed to load configuration");

    let ctx = AppContext::new(
        &config.database.path,
        AuthConfig {
            token: config.auth.jwt_secret.clone(),
            expire: std::time::Duration::from_secs(config.auth.jwt_expire_seconds),
        },
        config.external.tmdb_token.clone(),
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
