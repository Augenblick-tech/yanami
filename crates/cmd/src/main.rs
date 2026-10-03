use anime::entity::model::AnimeSeriesMetadata;
use mimalloc::MiMalloc;
use web::app_ctx::{AppContext, AuthConfig};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,librqbit=error,librqbit_dht=error")
    });
    let (filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    init(reload_handle).await;
}

async fn init(
    reload_handle: tracing_subscriber::reload::Handle<
        tracing_subscriber::EnvFilter,
        tracing_subscriber::Registry,
    >,
) {
    let config = cmd::config::AppConfig::load().expect("failed to load configuration");

    let reloader: std::sync::Arc<dyn Fn(String) -> Result<(), String> + Send + Sync> =
        std::sync::Arc::new(move |filter: String| {
            reload_handle.reload(&filter).map_err(|e| e.to_string())
        });

    let ctx = AppContext::new(
        &config.database.path,
        AuthConfig {
            token: config.auth.jwt_secret.clone(),
            expire: std::time::Duration::from_secs(config.auth.jwt_expire_seconds),
            crypto_secret: config.auth.crypto_secret.clone(),
        },
        config.external.tmdb_token.clone(),
        config.data_dir.clone(),
        reloader,
    )
    .await;
    let ctx = std::sync::Arc::new(ctx);

    ctx.init_database().await;

    backfill_series(&ctx).await;

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

/// 补全系列数据：已经有 TMDB 系列身份、却还没有系列展示信息的番剧，启动时补齐一次。
/// 取数失败的番剧跳过（不写任何占位数据），下次启动会重新判断。
async fn backfill_series(ctx: &AppContext) {
    let tmdb_ids = match ctx.repo.anime_repo.list_missing_series_ids().await {
        Ok(tmdb_ids) => tmdb_ids,
        Err(e) => {
            tracing::error!("backfill series query missing failed, {}", e);
            return;
        }
    };

    if tmdb_ids.is_empty() {
        return;
    }

    tracing::info!("backfill series started, count = {}", tmdb_ids.len());

    let mut series = Vec::with_capacity(tmdb_ids.len());
    for tmdb_id in tmdb_ids {
        let detail = match ctx.caps.tmdb_client.get_tv_detail(tmdb_id).await {
            Ok(detail) => detail,
            Err(e) => {
                tracing::error!("backfill series fetch failed, tmdb_id = {}, {}", tmdb_id, e);
                continue;
            }
        };

        let metadata = AnimeSeriesMetadata {
            origin_name: detail.inner.original_name,
            cn_name: detail.inner.name,
            desc: detail.inner.overview,
            air_date: detail.inner.first_air_date.unwrap_or_default(),
            genres: detail.genres.into_iter().map(|i| i.name).collect(),
        };

        if metadata.origin_name.is_empty() {
            tracing::error!("backfill series metadata is empty, tmdb_id = {}", tmdb_id);
            continue;
        }

        series.push((tmdb_id, metadata));
    }

    if let Err(e) = ctx.repo.anime_repo.save_series(&series).await {
        tracing::error!("backfill series save failed, {}", e);
    }
}
