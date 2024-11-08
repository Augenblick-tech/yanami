use std::sync::Arc;

use anna::{
    anime::tracker::AnimeTracker, bgm::bangumi::BGM, rss::client::Client, tmdb::client::TMDB,
};
use common::auth;
use mimalloc::MiMalloc;
use model::user::{UserCharacter, UserEntity};
use orm::sqlx;
use tokio::spawn;
use tracing_subscriber::{
    fmt::{self},
    EnvFilter,
};

use yanami::{
    config::Config,
    route::{route, Service},
    task::tasker::Tasker,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let config = Config::load().unwrap();
    tracing::subscriber::set_global_default(
        fmt::Subscriber::builder()
            .with_env_filter(EnvFilter::new(format!(
                "yanami={}",
                config.mode.clone().unwrap_or_else(|| "info".to_string())
            )))
            // .with_max_level(LevelFilter::TRACE)
            // .with_test_writer()
            .finish(),
    )
    .unwrap();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            init(config).await;
        })
}

async fn init(config: Config) {
    auth::init(config.key.clone().unwrap().to_owned());
    tracing::info!("listening on {}", &config.addr.clone().unwrap());

    // let redb = Arc::new(match ReDB::new(config.db_path.unwrap().to_string()) {
    //     Ok(db) => db,
    //     Err(err) => panic!("init db failed, {}", err),
    // });
    let redb = Arc::new(
        match sqlx::SqlxDB::new(config.db_path.expect("failed connect db").as_str()).await {
            Ok(db) => db,
            Err(err) => panic!("init db failed, {}", err),
        },
    );
    let rss_http_client = Arc::new(Client::new());
    let tasker = Tasker::new(
        redb.clone(),
        rss_http_client.clone(),
        redb.clone(),
        Arc::new(AnimeTracker::new(
            TMDB::new(config.tmdb_token.clone().unwrap().as_str()).expect("new tmdb client failed"),
            BGM::new().expect("new bgm client failed"),
        )),
        redb.clone(),
        redb.clone(),
    );
    let service = Service::new(
        redb.clone(),
        redb.clone(),
        redb.clone(),
        redb.clone(),
        redb.clone(),
        rss_http_client,
        redb,
    );

    if service.db.is_empty().await.expect("check table") {
        tracing::info!("create admin user, username: moexco, password: 123456");
        service
            .user_db
            .create_user(UserEntity {
                id: 10001,
                username: String::from("moexco"),
                password: UserEntity::into_sha256_pwd("123456".to_string()),
                chatacter: UserCharacter::Admin,
            })
            .await
            .expect("create admin user failed");
    }

    spawn(async move {
        tasker.run().await;
    });
    let app = route(service);
    let listenter = tokio::net::TcpListener::bind(config.addr.unwrap().to_string())
        .await
        .unwrap();
    axum::serve(listenter, app).await.unwrap();
}
