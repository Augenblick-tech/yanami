use std::fs;

use anyhow::Error;
use clap::Parser;
use serde::Deserialize;

#[derive(clap::Parser, Debug, Deserialize, Default)]
pub struct Config {
    #[clap(short, long, env)]
    pub addr: Option<String>,
    #[clap(short, long, env)]
    pub mode: Option<String>,
    #[clap(short, long, env)]
    pub key: Option<String>,
    #[clap(short, long, env)]
    pub db_path: Option<String>,
    #[clap(short, long, env)]
    pub tmdb_token: Option<String>,
    #[clap(short, long, env)]
    pub config: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        let args = Config::parse();
        let config = if let Some(config_path) = args.config.clone() {
            let config_content =
                fs::read_to_string(config_path).expect("Failed to read config file");
            toml::from_str::<Config>(&config_content).expect("Failed to parse config file")
        } else {
            Config::default()
        };
        let mode = args
            .mode
            .or(config.mode.or(Some("info".to_string())))
            .unwrap();

        let mode = if mode.eq("debug") || mode.eq("warn") {
            mode
        } else {
            "info".to_string()
        };

        Ok(Config {
            addr: Some(args.addr.or(config.addr).expect("addr is required")),
            db_path: Some(
                args.db_path
                    .or(config.db_path)
                    .expect("db_path is required"),
            ),
            key: Some(args.key.or(config.key).expect("key is required")),
            mode: Some(mode),
            tmdb_token: Some(
                args.tmdb_token
                    .or(config.tmdb_token)
                    .expect("tmdb_token is required"),
            ),
            config: None,
        })
    }
}
