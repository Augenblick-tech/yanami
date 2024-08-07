use core::panic;
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use yanami::{
    common::auth,
    config::Config,
    provider::db::redb::ReDB,
    route::{route, ServiceRegister},
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

    let service = ServiceRegister::new(Arc::new(match ReDB::new(config.db_path.to_string()) {
        Ok(db) => db,
        Err(err) => panic!("init db failed, {}", err),
    }));
    let app = route(service);
    let listenter = tokio::net::TcpListener::bind(config.addr.to_string())
        .await
        .unwrap();
    axum::serve(listenter, app).await.unwrap();
}
