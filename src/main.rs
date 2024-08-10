use core::panic;
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use yanami::{
    common::auth::{self, UserCharacter},
    config::Config,
    models::user::UserEntity,
    provider::db::redb::ReDB,
    route::{route, Service},
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

    let config = Arc::new(Config::parse());

    auth::init(config.key.to_owned());
    tracing::debug!("listening on {}", &config.addr);

    let redb = Arc::new(match ReDB::new(config.db_path.to_string()) {
        Ok(db) => db,
        Err(err) => panic!("init db failed, {}", err),
    });
    let service = Service::new(redb.clone(), redb.clone(), redb);

    if service.db.is_empty().expect("check table") {
        service
            .user
            .create_user(UserEntity {
                id: 0,
                username: String::from("moexco"),
                password: UserEntity::into_sha256_pwd("123456".to_string()),
                chatacter: UserCharacter::Admin,
            })
            .expect("create admin user failed");
    }

    let app = route(service);
    let listenter = tokio::net::TcpListener::bind(config.addr.to_string())
        .await
        .unwrap();
    axum::serve(listenter, app).await.unwrap();
}
