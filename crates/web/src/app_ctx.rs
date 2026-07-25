use std::{sync::Arc, time::Duration};

use anime::{
    entity::{anime_source::AnimeSources, animes::Animes},
    infra::{
        anime_source::{bgm::client::BgmClient, tmdb::client::TmdbClient},
        repository::client::AnimeSqliteClient,
    },
};
use dashmap::DashMap;
use feed::{
    entity::feeds::Feeds,
    infra::{
        feed::HttpFeedFetcher, feed_access_policy::BackoffPolicy,
        repository::client::FeedSqliteClient,
    },
};
use regex::Regex;
use reqwest::{
    Client,
    header::{HeaderMap, USER_AGENT},
};
use resource::{entity::resources::Resources, infra::repository::client::ResourceSqliteClient};
use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use subscription::{
    entity::{rules::Rules, search_mandates::SearchMandates, sub_animes::SubAnimes},
    infra::{
        regex::RegexRuleMatcher,
        repository::client::{RuleSqliteClient, SearchMandateSqliteClient, SubAnimeSqliteClient},
    },
};
use user::{
    entity::users::Users,
    infra::{
        crypto::AesCryptoProvider, downloader_manager::DownloaderManager,
        repository::client::UserSqliteClient,
    },
};

use crate::{middleware::auth::JwtDecoder, token_issuer::JwtAccessTokenIssuer};

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub token: String,
    pub expire: Duration,
    pub crypto_secret: String,
}

#[derive(Clone)]
pub struct Base {
    pub pool: Pool<Sqlite>,
    pub regex_cache: Arc<DashMap<String, Regex>>,
    pub http_client: Client,
    pub auth_config: AuthConfig,
    pub tmdb_token: String,
}

#[derive(Clone)]
pub struct Repo {
    pub anime_repo: Arc<AnimeSqliteClient>,
    pub rule_repo: Arc<RuleSqliteClient>,
    pub sub_anime_repo: Arc<SubAnimeSqliteClient>,
    pub user_repo: Arc<UserSqliteClient>,
    pub feed_repo: Arc<FeedSqliteClient>,
    pub res_repo: Arc<ResourceSqliteClient>,
    pub mandate_repo: Arc<SearchMandateSqliteClient>,
}

pub type LogLevelReloader = Arc<dyn Fn(String) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct Caps {
    pub matcher: RegexRuleMatcher,
    pub downloader_manager: Arc<DownloaderManager>,
    pub access_policy: BackoffPolicy,
    pub feed_fetcher: HttpFeedFetcher,
    pub tmdb_client: Arc<TmdbClient>,
    pub bgm_client: Arc<BgmClient>,
    pub jwt: Arc<JwtAccessTokenIssuer>,
    pub jwt_decoder: Arc<JwtDecoder>,
    pub log_level_reloader: LogLevelReloader,
    pub crypto_provider: Arc<AesCryptoProvider>,
}

#[derive(Clone)]
pub struct Roots {
    pub users: Users,
    pub animes: Animes,
    pub sub_animes: SubAnimes,
    pub feeds: Feeds,
    pub resources: Resources,
    pub search_mandates: SearchMandates,
    pub anime_source: AnimeSources,
    pub rules: Rules,
}

#[derive(Clone)]
pub struct Queries {
    pub anime_view: crate::query::anime_view::AnimeViewQuery,
    pub stat_view: crate::query::stat_view::StatQuery,
}

#[derive(Clone)]
pub struct AppContext {
    base: Base,
    repo: Repo,
    pub caps: Caps,
    pub roots: Roots,
    pub queries: Queries,
}

impl AppContext {
    pub async fn new(
        db_filename: &str,
        auth_config: AuthConfig,
        tmdb_token: String,
        log_level_reloader: LogLevelReloader,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            "yanami/1.0"
                .parse()
                .expect("build http client default header failed"),
        );

        let base = Base {
            pool: SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(db_filename)
                        .create_if_missing(true)
                        .page_size(0)
                        .pragma("temp_store", "FILE")
                        .pragma("cache_size", "0"),
                )
                .await
                .expect("connect db failed"),
            regex_cache: Arc::new(DashMap::new()),
            http_client: Client::builder()
                .default_headers(headers)
                .timeout(Duration::from_secs(30))
                .build()
                .expect("build http client failed"),
            auth_config,
            tmdb_token,
        };

        let (repo, caps) = Self::init_repo_and_caps(&base, log_level_reloader);

        let roots = Self::init_roots(&repo, &caps);
        let queries = Queries {
            anime_view: crate::query::anime_view::AnimeViewQuery::new(base.pool.clone()),
            stat_view: crate::query::stat_view::StatQuery::new(base.pool.clone()),
        };

        Self {
            base,
            repo,
            caps,
            roots,
            queries,
        }
    }

    fn init_repo_and_caps(base: &Base, log_level_reloader: LogLevelReloader) -> (Repo, Caps) {
        let matcher = RegexRuleMatcher::new(base.regex_cache.clone());
        let downloader_manager = Arc::new(DownloaderManager::new(base.http_client.clone()));
        let access_policy = BackoffPolicy::new();

        let tmdb_client = Arc::new(TmdbClient::new(&base.tmdb_token, base.http_client.clone()));
        let bgm_client = Arc::new(BgmClient::new(
            base.http_client.clone(),
            (*tmdb_client).clone(),
        ));

        let jwt = Arc::new(
            JwtAccessTokenIssuer::new(
                &base.auth_config.token,
                base.auth_config.expire.as_secs() as i64,
            )
            .expect("init jwt failed"),
        );

        let jwt_decoder = Arc::new(JwtDecoder::new(&base.auth_config.token));
        let crypto_provider = Arc::new(AesCryptoProvider::new(&base.auth_config.crypto_secret));

        let anime_repo = Arc::new(AnimeSqliteClient::new(base.pool.clone()));
        let rule_repo = Arc::new(RuleSqliteClient::new(base.pool.clone(), matcher.clone()));
        let sub_anime_repo = Arc::new(SubAnimeSqliteClient::new(base.pool.clone()));
        let user_repo = Arc::new(UserSqliteClient::new(base.pool.clone()));
        let feed_repo = Arc::new(FeedSqliteClient::new(base.pool.clone()));
        let res_repo = Arc::new(ResourceSqliteClient::new(base.pool.clone()));
        let mandate_repo = Arc::new(SearchMandateSqliteClient::new(base.pool.clone()));

        let feed_fetcher = HttpFeedFetcher::new(base.http_client.clone(), res_repo.clone());
        (
            Repo {
                anime_repo,
                rule_repo,
                sub_anime_repo,
                user_repo,
                feed_repo,
                res_repo,
                mandate_repo,
            },
            Caps {
                matcher,
                downloader_manager,
                access_policy,
                feed_fetcher,
                tmdb_client,
                bgm_client,
                jwt,
                jwt_decoder,
                log_level_reloader,
                crypto_provider,
            },
        )
    }

    fn init_roots(repo: &Repo, caps: &Caps) -> Roots {
        let fetcher = Arc::new(caps.feed_fetcher.clone());
        let access_policy = Arc::new(caps.access_policy.clone());
        let matcher = Arc::new(caps.matcher.clone());
        Roots {
            users: Users::new(
                repo.user_repo.clone(),
                caps.downloader_manager.clone(),
                caps.crypto_provider.clone(),
            ),
            animes: Animes::new(repo.anime_repo.clone()),
            sub_animes: SubAnimes::new(
                repo.sub_anime_repo.clone(),
                repo.rule_repo.clone(),
                matcher.clone(),
            ),
            feeds: Feeds::new(
                repo.feed_repo.clone(),
                fetcher.clone(),
                access_policy.clone(),
            ),
            resources: Resources::new(repo.res_repo.clone()),
            search_mandates: SearchMandates::new(
                repo.mandate_repo.clone(),
                fetcher.clone(),
                access_policy.clone(),
            ),
            anime_source: AnimeSources::new(caps.bgm_client.clone(), vec![caps.bgm_client.clone()]),
            rules: Rules::new(repo.rule_repo.clone(), matcher.clone()),
        }
    }

    pub async fn init_database(&self) {
        let mut tx = self
            .base
            .pool
            .begin()
            .await
            .expect("init database begin tx failed");

        self.repo
            .anime_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init anime database failed");
        self.repo
            .rule_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init rule database failed");
        self.repo
            .sub_anime_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init sub_anime database failed");
        self.repo
            .user_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init user database failed");
        self.repo
            .feed_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init feed database failed");
        self.repo
            .res_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init res database failed");
        self.repo
            .mandate_repo
            .init_with_tx(&mut tx)
            .await
            .expect("init mandate database failed");

        tx.commit().await.expect("init database commit failed");

        self.roots
            .users
            .init_admin_user()
            .await
            .expect("init admin user failed")
    }
}
