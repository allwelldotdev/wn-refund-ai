//! Process configuration from the environment.

use std::net::SocketAddr;

use ai::AiConfig;
use anyhow::Context;

/// The value `.env.example` ships with.
const PLACEHOLDER_API_KEY: &str = "sk-or-v1-replace-me";

pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub ai: AiConfig,
    openrouter_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .context("BIND_ADDR must be host:port")?;
        let ai = AiConfig::load().context("invalid AI_* model configuration")?;
        let openrouter_api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty());
        Ok(Config {
            database_url,
            bind_addr,
            ai,
            openrouter_api_key,
        })
    }

    /// Serving needs the key; `refund-api seed` does not.
    pub fn openrouter_api_key(&self) -> anyhow::Result<&str> {
        match self.openrouter_api_key.as_deref() {
            None => anyhow::bail!(
                "OPENROUTER_API_KEY is not set. Put your OpenRouter key in .env (see .env.example)."
            ),
            Some(PLACEHOLDER_API_KEY) => anyhow::bail!(
                "OPENROUTER_API_KEY is still the .env.example placeholder. Replace it with your OpenRouter key."
            ),
            Some(key) => Ok(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_key(key: Option<&str>) -> Config {
        Config {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ai: AiConfig::load().unwrap(),
            openrouter_api_key: key.map(str::to_owned),
        }
    }

    #[test]
    fn serving_requires_a_real_key() {
        let missing = with_key(None).openrouter_api_key().unwrap_err();
        assert!(missing.to_string().contains("not set"), "{missing}");
        let placeholder = with_key(Some(PLACEHOLDER_API_KEY))
            .openrouter_api_key()
            .unwrap_err();
        assert!(
            placeholder.to_string().contains("placeholder"),
            "{placeholder}"
        );
        assert_eq!(
            with_key(Some("sk-or-v1-abc")).openrouter_api_key().unwrap(),
            "sk-or-v1-abc"
        );
    }
}
