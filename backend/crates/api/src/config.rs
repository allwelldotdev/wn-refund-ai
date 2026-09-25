//! Process configuration from the environment.

use std::net::SocketAddr;

use ai::AiConfig;
use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub ai: AiConfig,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("BIND_ADDR must be host:port")?;
        let ai = AiConfig::load().context("invalid AI_* model configuration")?;
        Ok(Config {
            database_url,
            bind_addr,
            ai,
        })
    }
}
