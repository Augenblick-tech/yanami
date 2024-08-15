use core::panic;
use std::sync::Arc;

use anna::{anime::anime::AnimeTracker, bgm::bgm::BGM, rss::rss::RssHttpClient, tmdb::tmdb::TMDB};
use tokio::spawn;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use yanami::{
    common::auth::{self, UserCharacter},
    config::Config,
    models::user::UserEntity,
    provider::db::redb::ReDB,
    route::{route, Service},
    task::task::Tasker,
};

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yanami=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let config = Config::load().unwrap();

    auth::init(config.key.clone().unwrap().to_owned());
    tracing::debug!("listening on {}", &config.addr.clone().unwrap());

    let redb = Arc::new(match ReDB::new(config.db_path.unwrap().to_string()) {
        Ok(db) => db,
        Err(err) => panic!("init db failed, {}", err),
    });
    let rss_http_client = Arc::new(RssHttpClient::new());
    let tasker = Tasker::new(
        redb.clone(),
        rss_http_client.clone(),
        redb.clone(),
        Arc::new(AnimeTracker::new(
            TMDB::new(config.tmdb_token.clone().unwrap().as_str()).expect("new tmdb client failed"),
            BGM::new().expect("new bgm client failed"),
        )),
        redb.clone(),
    );
    spawn(async move {
        tasker.run().await;
    });
    let service = Service::new(
        redb.clone(),
        redb.clone(),
        redb.clone(),
        redb.clone(),
        redb.clone(),
        rss_http_client,
        redb,
    );

    if service.db.is_empty().expect("check table") {
        service
            .user_db
            .create_user(UserEntity {
                id: 0,
                username: String::from("moexco"),
                password: UserEntity::into_sha256_pwd("123456".to_string()),
                chatacter: UserCharacter::Admin,
            })
            .expect("create admin user failed");
    }

    let app = route(service);
    let listenter = tokio::net::TcpListener::bind(config.addr.unwrap().to_string())
        .await
        .unwrap();
    axum::serve(listenter, app).await.unwrap();
}
