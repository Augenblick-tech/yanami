use std::sync::Arc;

use anime::{animes::Animes, source::AnimeSourceFactory, AnimeCaps};
use subscription::missing_episodes::MissingEpisodeChecker;
use infra::{anime_source::BangumiSingleSource, bangumi::BangumiClient, tmdb::TmdbClient};
use anyhow::Result;
use axum::serve;

use domain::space::SpaceId;
use domain::user::UserId;
use feed::{Feeds as FeedsContext, Resources as ResourceContext};
use infra::{
    db::SqliteDb,
    noop_download::NoopDownloadDriver,
    qbit::{LiveQbitProfileVerifier, QbitDownloadDriver, UserBoundQbitDownloadDriver},
    rss::HttpFeedFetcher,
    rule_runtime::{CachingRegexProvider, CachingRuleRuntime},
    user::{
        JwtAccessTokenIssuer, LegacySha256PasswordService, SqliteUserIdGenerator, SystemEpochClock,
    },
};
use job::{ensure_unique_job_names, InMemoryJobGuard, TokioScheduler};
use rule::Rules;
use service::{
    anime::service::AnimeService,
    download::{
        runtime::{
            CachingUserDownloadDriverResolver, CompositeUserDownloadRuntimeCacheInvalidator,
            InvalidateUserDownloadRuntime, ResolveUserDownloadDriver, RoutingUserDownloadExecutor,
            UserDownloadDriver, VerifyQbitProfile,
        },
        service::DownloadService,
        user_actions::UserDownload,
    },
    feed::service::FeedService,
    job::{
        CheckMissingEpisodesJob, FetchResourcesJob, Job, SyncAnimeCalendarJob,
    },
    rule::service::RuleService,
    space::service::SpaceService,
    subscription::pool_consumer_handler::SearchPoolHandler,
    subscription::resume_handler::ResumeCompletedSubscriptions,
    subscription::service::{SubscriptionService, SubscriptionServiceDependencies},
    system::service::SystemService,
    user::service::UserService,
};
use space::Spaces;
use subscription::SubscriptionAnimes;
use tracing_subscriber::{fmt, EnvFilter};
use user::users::Users;

use service::download::downloads::UserDownloads;

use crate::{
    config::SchedulerConfig,
    http::{auth::JwtDecoder, router::build_router, state::AppState},
    matched_resource_action::DownloadMatchedResourceAction,
    metadata::{build_metadata_sources, normalize_sqlite_db_url},
};

pub async fn run(config: SchedulerConfig) -> Result<()> {
    init_tracing(&config.mode, config.log_file.as_deref())?;

    let database = Arc::new(
        SqliteDb::connect(
            normalize_sqlite_db_url(&config.db_path).as_str(),
            &config.key,
        )
        .await?,
    );
    let runtime = build_runtime(database.clone(), &config).await?;
    let scheduler = TokioScheduler::new(InMemoryJobGuard::new());
    let sync_job = build_sync_anime_calendar_job(
        runtime.anime_service.clone(),
        runtime.subscription_service.clone(),
        config.tmdb_token.clone(),
        config.sources.clone(),
    )
    .await?;
    let missing_eps_job =
        build_check_missing_episodes_job(runtime.subscription_service.clone()).await?;
    let fetch_resources_job =
        build_fetch_resources_job(runtime.subscription_service.clone()).await?;

    let job_names = build_job_names(
        sync_job.name(),
        missing_eps_job.name(),
        fetch_resources_job.name(),
    );
    ensure_unique_job_names(job_names).unwrap_or_else(|error| {
        panic!("failed to initialize scheduled jobs: error={error}, jobs={job_names:?}")
    });

    let sync_handle = scheduler.spawn_scheduled(sync_job, config.jobs.sync_anime_calendar.clone());
    let missing_eps_handle =
        scheduler.spawn_scheduled(missing_eps_job, config.jobs.check_missing_episodes.clone());
    let fetch_resources_handle =
        scheduler.spawn_scheduled(fetch_resources_job, config.jobs.fetch_resources.clone());

    let pool_handler = Arc::new(SearchPoolHandler::new(
        runtime.subscription_service.clone(),
        database.clone(),
    ));
    crate::pool_consumer::spawn_pool_consumer(
        database.clone(),
        runtime.http_feed.clone(),
        vec![pool_handler],
    );

    let app_state = Arc::new(build_http_state(runtime, config.key.as_str()));
    let router = build_router(app_state);
    let listener = tokio::net::TcpListener::bind(&config.addr).await?;

    tracing::info!(
        "yanami started, addr={}, sources={:?}, noop_download_driver={}",
        config.addr,
        config.sources,
        config.download.noop_enabled,
    );
    serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    abort_handle(sync_handle);
    abort_handle(missing_eps_handle);
    abort_handle(fetch_resources_handle);
    tracing::info!("yanami stopped");
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!("failed to listen for ctrl_c: {error}");
    }
}

fn init_tracing(mode: &str, log_file: Option<&str>) -> Result<()> {
    use std::sync::Arc;
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;

    let level = normalize_log_mode(mode);
    let filter_spec = format!(
        "cmd={level},service={level},anime={level},infra={level},feed={level},subscription={level}"
    );

    if let Some(path) = log_file {
        let file = Arc::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        );
        let file_clone = file.clone();

        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(
                    fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_filter(EnvFilter::new(&filter_spec)),
                )
                .with(
                    fmt::layer()
                        .with_writer(move || file_clone.try_clone().expect("log file clone"))
                        .with_filter(EnvFilter::new(&filter_spec)),
                ),
        )?;
    } else {
        tracing::subscriber::set_global_default(
            fmt::Subscriber::builder()
                .with_env_filter(EnvFilter::new(&filter_spec))
                .finish(),
        )?;
    }
    Ok(())
}

fn normalize_log_mode(mode: &str) -> &str {
    match mode {
        "trace" | "debug" | "warn" | "info" => mode,
        _ => "info",
    }
}

fn build_job_names(
    sync_anime_calendar: &'static str,
    check_missing_episodes: &'static str,
    fetch_resources: &'static str,
) -> [&'static str; 3] {
    [
        sync_anime_calendar,
        check_missing_episodes,
        fetch_resources,
    ]
}

fn abort_handle(handle: Option<tokio::task::JoinHandle<()>>) {
    if let Some(handle) = handle {
        handle.abort();
    }
}

fn build_run_matched_resource(
    action: Arc<DownloadMatchedResourceAction>,
) -> Arc<subscription::action::RunMatchedResource> {
    Arc::new(move |resource| {
        let action = action.clone();
        let future: std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<(), subscription::shared::error::ApplicationError>,
                    > + Send,
            >,
        > = Box::pin(async move { action.run(resource).await });
        future
    })
}

fn build_resolve_driver_key(
    resolver: Arc<CachingUserDownloadDriverResolver>,
) -> Arc<ResolveUserDownloadDriver> {
    Arc::new(move |user_id| {
        let resolver = resolver.clone();
        Box::pin(async move { resolver.resolve_driver_key(user_id).await })
    })
}

fn build_invalidate_user_runtime(
    invalidator: Arc<CompositeUserDownloadRuntimeCacheInvalidator>,
) -> Arc<InvalidateUserDownloadRuntime> {
    Arc::new(move |user_id| invalidator.invalidate_user_runtime(user_id))
}

fn build_verify_qbit_profile(verifier: Arc<LiveQbitProfileVerifier>) -> Arc<VerifyQbitProfile> {
    Arc::new(move |profile| {
        let verifier = verifier.clone();
        Box::pin(async move { verifier.verify_qbit_profile(&profile).await })
    })
}

async fn build_sync_anime_calendar_job(
    anime_service: Arc<AnimeService>,
    subscription_service: Arc<SubscriptionService>,
    tmdb_token: String,
    sources: Vec<crate::metadata::MetadataSourceKind>,
) -> Result<Arc<SyncAnimeCalendarJob>> {
    let source_factory: Arc<AnimeSourceFactory> = Arc::new(move || {
        let sources = if sources.is_empty() {
            crate::metadata::normalize_sources(None, None, None)
        } else {
            sources.clone()
        };
        build_metadata_sources(&tmdb_token, &sources).map_err(|error| {
            domain::shared::error::DomainError::external("anime source build failed", error)
        })
    });
    Ok(Arc::new(SyncAnimeCalendarJob::new(
        anime_service,
        subscription_service,
        source_factory,
    )))
}

async fn build_check_missing_episodes_job(
    subscription_service: Arc<SubscriptionService>,
) -> Result<Arc<CheckMissingEpisodesJob>> {
    Ok(Arc::new(CheckMissingEpisodesJob::new(subscription_service)))
}

async fn build_fetch_resources_job(
    subscription_service: Arc<SubscriptionService>,
) -> Result<Arc<FetchResourcesJob>> {
    Ok(Arc::new(FetchResourcesJob::new(subscription_service)))
}

pub(crate) async fn build_runtime(
    database: Arc<SqliteDb>,
    config: &SchedulerConfig,
) -> Result<RuntimeServices> {
    let rule_runtime = Arc::new(CachingRuleRuntime::new(database.clone()));
    let qbit_dispatcher = Arc::new(QbitDownloadDriver::new());
    let qbit_driver = Arc::new(UserBoundQbitDownloadDriver::from_sqlite(
        database.clone(),
        qbit_dispatcher,
        128,
    ));
    let download_resolver = Arc::new(CachingUserDownloadDriverResolver::new(
        database.clone(),
        256,
    ));
    let download_cache_invalidator =
        Arc::new(CompositeUserDownloadRuntimeCacheInvalidator::new(vec![
            Arc::new({
                let download_resolver = download_resolver.clone();
                move |user_id| download_resolver.invalidate_user_runtime(user_id)
            }),
            Arc::new({
                let invalidator = qbit_driver.cache_invalidator();
                move |user_id| invalidator.invalidate_user_runtime(user_id)
            }),
        ]));
    let mut download_drivers: Vec<Arc<dyn UserDownloadDriver>> = vec![qbit_driver.clone()];
    if config.download.noop_enabled {
        download_drivers.push(Arc::new(NoopDownloadDriver));
    }
    let download_executor = Arc::new(RoutingUserDownloadExecutor::new(
        build_resolve_driver_key(download_resolver.clone()),
        download_drivers,
    )?);
    let user_download = Arc::new(UserDownload::new(download_executor.clone()));
    let matched_resource_action =
        Arc::new(DownloadMatchedResourceAction::new(user_download.clone()));
    let http_feed = Arc::new(HttpFeedFetcher::new()?);
    let feed_source_resolver = build_feed_source_resolver(http_feed.clone());
    let feeds_impl = Arc::new(FeedsContext::new(
        database.clone(),
        feed_source_resolver.clone(),
        http_feed.clone(),
    ));
    let rule_caps = rule::RuleCaps {
        writer: database.clone(),
    };
    let rules = Arc::new(Rules::new(
        rule_caps,
        database.clone(),
        Arc::new(CachingRegexProvider::default()),
    ));
    let spaces_impl = Arc::new(Spaces::new(
        database.clone(),
        database.clone() as Arc<dyn domain::shared::identifier::IdSequence>,
    ));
    let resources = Arc::new(ResourceContext::new(
        database.clone(),
        Arc::new(SystemEpochClock) as Arc<dyn user::gateway::EpochClock>,
        feeds_impl.clone(),
    ));
    let animes = build_animes(database.clone());

    let password_service = Arc::new(LegacySha256PasswordService);
    let user_ids = Arc::new(SqliteUserIdGenerator::new(database.clone()));
    let token_issuer = Arc::new(JwtAccessTokenIssuer::new(
        &config.key,
        config.token_ttl_seconds,
    )?);
    let user_accounts = Arc::new(Users::new(
        database.clone(),
        password_service.clone(),
        user_ids.clone(),
    ));
    let initialization = SystemService::new(
        database.clone(),
        database.clone(),
        user_accounts.clone(),
        spaces_impl.clone(),
    )
    .ensure_initialized("moexco", "123456")
    .await?;
    let single_anime_source = Arc::new(BangumiSingleSource::new(
        BangumiClient::new()?,
        TmdbClient::new(&config.tmdb_token)?,
    )) as Arc<dyn anime::source::SingleAnimeSource>;

    let subscription_caps = subscription::SubscriptionCaps {
        toggle: database.clone(),
        match_writer: database.clone(),
        search: database.clone(),
    };
    let subscriptions = Arc::new(SubscriptionAnimes::new(subscription_caps, database.clone(), database.clone()));
    let subscription_service =
        Arc::new(SubscriptionService::new(SubscriptionServiceDependencies {
            search_pool: database.clone(),
            biz_factory: database.clone(),
            subscriptions: subscriptions.clone(),
            spaces: spaces_impl.clone(),
            animes: animes.clone(),
            feeds: feeds_impl.clone(),
            rules: rules.clone(),
            resources: resources.clone(),
            missing_episode_policy: Arc::new(MissingEpisodeChecker),
            run_matched_resource: build_run_matched_resource(matched_resource_action),
        }));
    let resume_handler = Arc::new(ResumeCompletedSubscriptions::new(
        animes.clone(),
        subscriptions.clone(),
    ));
    let anime_service = Arc::new(AnimeService::new(
        animes,
        subscriptions.clone(),
        subscription_service.clone(),
        vec![resume_handler],
    ));
    Ok(RuntimeServices {
        database,
        http_feed,
        token_issuer,
        admin_user_id: initialization.admin_user_id,
        admin_space_id: initialization.admin_space_id,
        rule_runtime,
        download_cache_invalidator,
        download_executor,
        user_accounts,
        feeds: feeds_impl,
        rules: rules.clone(),
        spaces: spaces_impl,
        subscription_service,
        anime_service,
        single_anime_source,
    })
}

fn build_feed_source_resolver(
    http_feed: Arc<HttpFeedFetcher>,
) -> Arc<feed::contracts::ResolveFeedSource> {
    Arc::new(move |source: domain::feed::FeedSource| {
        let http_feed = http_feed.clone();
        let future: std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            feed::contracts::ResolvedFeedSource,
                            domain::shared::error::DomainError,
                        >,
                    > + Send,
            >,
        > = Box::pin(async move { http_feed.resolve_source(&source).await });
        future
    })
}

pub(crate) struct RuntimeServices {
    pub(crate) database: Arc<SqliteDb>,
    pub(crate) http_feed: Arc<HttpFeedFetcher>,
    token_issuer: Arc<JwtAccessTokenIssuer>,
    admin_user_id: UserId,
    admin_space_id: SpaceId,
    rule_runtime: Arc<CachingRuleRuntime>,
    download_cache_invalidator: Arc<CompositeUserDownloadRuntimeCacheInvalidator>,
    download_executor: Arc<RoutingUserDownloadExecutor>,
    user_accounts: Arc<Users>,
    feeds: Arc<FeedsContext>,
    rules: Arc<Rules>,
    spaces: Arc<Spaces>,
    subscription_service: Arc<SubscriptionService>,
    anime_service: Arc<AnimeService>,
    single_anime_source: Arc<dyn anime::source::SingleAnimeSource>,
}

fn build_http_state(runtime: RuntimeServices, application_key: &str) -> AppState {
    build_http_state_with_qbit_verifier(
        runtime,
        application_key,
        build_verify_qbit_profile(Arc::new(LiveQbitProfileVerifier)),
    )
}

pub(crate) fn build_http_state_with_qbit_verifier(
    runtime: RuntimeServices,
    application_key: &str,
    qbit_profile_verifier: Arc<VerifyQbitProfile>,
) -> AppState {
    let rule_service = Arc::new(RuleService::new(runtime.rules.clone(), {
        let runtime = runtime.rule_runtime.clone();
        Arc::new(move |space_id| runtime.invalidate_space_rules(space_id))
    }));
    let user_download = Arc::new(UserDownload::new(runtime.download_executor.clone()));
    let available_drivers: Vec<String> = runtime
        .download_executor
        .driver_keys()
        .into_iter()
        .map(|k| k.to_string())
        .collect();
    let downloads = Arc::new(UserDownloads::new(
        runtime.user_accounts.clone(),
        runtime.database.clone(),
        runtime.database.clone(),
        qbit_profile_verifier,
        build_invalidate_user_runtime(runtime.download_cache_invalidator.clone()),
        user_download,
        available_drivers,
    ));
    let download_service = Arc::new(DownloadService::new(downloads));
    let feed_service = Arc::new(FeedService::new(runtime.feeds.clone()));
    let space_service = Arc::new(SpaceService::new(runtime.spaces.clone()));

    AppState {
        auth: JwtDecoder::new(application_key),
        space_service,
        user_service: Arc::new(UserService::new(
            runtime.user_accounts.clone(),
            runtime.token_issuer.clone(),
        )),
        rule_service,
        feed_service,
        download_service,
        anime_service: runtime.anime_service,
        subscription_service: runtime.subscription_service,
        single_anime_source: runtime.single_anime_source,
        admin_user_id: runtime.admin_user_id,
        admin_space_id: runtime.admin_space_id,
    }
}

fn build_animes(database: Arc<SqliteDb>) -> Arc<anime::animes::Animes> {
    let caps = AnimeCaps {
        locker: database.clone(),
        metadata_updater: database.clone(),
    };
    Arc::new(Animes::new(caps, database.clone(), database.clone(), database))
}
