#[derive(clap::Parser)]
pub struct Config {
    #[clap(long, env)]
    pub addr: String,
    #[clap(long, env)]
    pub mode: String,
    #[clap(long, env)]
    pub key: String,
    #[clap(long, env)]
    pub db_path: String,
}
