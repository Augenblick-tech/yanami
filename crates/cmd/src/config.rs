use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// 配置文件路径
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,

    /// 绑定的主机地址
    #[arg(long)]
    pub host: Option<String>,

    /// 监听的端口
    #[arg(short, long)]
    pub port: Option<u16>,

    /// 数据库文件路径
    #[arg(long)]
    pub db_path: Option<String>,

    /// 用于签发 JWT Token 的密钥
    #[arg(long)]
    pub jwt_secret: Option<String>,

    /// 用于加密敏感信息的主密钥
    #[arg(long)]
    pub crypto_secret: Option<String>,

    /// TMDB API 读取权限的 Token
    #[arg(long)]
    pub tmdb_token: Option<String>,

    /// 工作/数据目录路径
    #[arg(long)]
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// HTTP 服务器配置
    pub server: ServerConfig,
    /// 数据库配置
    pub database: DatabaseConfig,
    /// 认证鉴权配置
    pub auth: AuthConfig,
    /// 外部第三方 API 配置
    pub external: ExternalConfig,
    /// 工作/数据目录路径
    pub data_dir: String,
}

impl AppConfig {
    /// 加载配置：优先读取命令行参数，否则从配置文件读取，最后使用默认值
    pub fn load() -> Result<Self> {
        let cli = CliArgs::parse();

        let mut config: AppConfig = if cli.config.exists() {
            let config_str = fs::read_to_string(&cli.config)
                .with_context(|| format!("failed to read config file {:?}", cli.config))?;
            toml::from_str(&config_str)
                .with_context(|| format!("failed to parse config file {:?}", cli.config))?
        } else {
            anyhow::bail!(
                "config file {:?} does not exist, please provide a complete config file",
                cli.config
            );
        };

        // 命令行参数覆盖配置文件
        if let Some(host) = cli.host {
            config.server.host = host;
        }
        if let Some(port) = cli.port {
            config.server.port = port;
        }
        if let Some(db_path) = cli.db_path {
            config.database.path = db_path;
        }
        if let Some(jwt_secret) = cli.jwt_secret {
            config.auth.jwt_secret = jwt_secret;
        }
        if let Some(crypto_secret) = cli.crypto_secret {
            config.auth.crypto_secret = crypto_secret;
        }
        if let Some(tmdb_token) = cli.tmdb_token {
            config.external.tmdb_token = tmdb_token;
        }
        if let Some(data_dir) = cli.data_dir {
            config.data_dir = data_dir;
        }
        if config.server.host.is_empty() {
            anyhow::bail!("server.host is required and cannot be empty");
        }
        if config.database.path.is_empty() {
            anyhow::bail!("database.path is required and cannot be empty");
        }
        if config.auth.jwt_secret.is_empty() {
            anyhow::bail!("auth.jwt_secret is required and cannot be empty");
        }
        if config.auth.crypto_secret.is_empty() {
            anyhow::bail!("auth.crypto_secret is required and cannot be empty");
        }
        if config.external.tmdb_token.is_empty() {
            anyhow::bail!("external.tmdb_token is required and cannot be empty");
        }
        if config.data_dir.is_empty() {
            anyhow::bail!("data_dir is required and cannot be empty");
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// 绑定的主机地址 (例如: "0.0.0.0" 或 "127.0.0.1")
    pub host: String,
    /// 监听的端口 (例如: 8080)
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// 数据库文件路径或连接字符串
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "yanami.db".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// 用于签发 JWT Token 的密钥
    pub jwt_secret: String,
    /// JWT Token 的过期时间 (单位：秒)
    pub jwt_expire_seconds: u64,
    /// 用于加密敏感信息 (如下载配置密码) 的主密钥
    pub crypto_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ExternalConfig {
    /// TMDB API 读取权限的 Token
    pub tmdb_token: String,
}
