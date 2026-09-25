//! `refund-api`         migrate, seed, then serve on BIND_ADDR (default 0.0.0.0:8080)
//! `refund-api seed`    migrate and seed, then exit

use std::sync::Arc;

use ai::OfflineAssistant;
use anyhow::Context;
use api::config::Config;
use api::rate_limit::RateLimiter;
use api::{AppState, build_router, prepare_database};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let config = Config::from_env()?;
    let db = db::connect(&config.database_url)
        .await
        .context("connecting to Postgres")?;
    let report = prepare_database(&db)
        .await
        .context("migrating and seeding")?;
    tracing::info!(?report, "database ready");

    match std::env::args().nth(1).as_deref() {
        Some("seed") => return Ok(()),
        Some(other) => anyhow::bail!("unknown command `{other}`; expected no argument or `seed`"),
        None => {}
    }

    // No LLM provider is wired yet: every stage call fails, so every request
    // fails closed to Escalated and waits for an admin (ADR-031).
    tracing::warn!("no LLM provider configured; every refund request will be escalated");
    let state = AppState {
        db,
        assistant: Arc::new(OfflineAssistant),
        ai: Arc::new(config.ai),
        rate: RateLimiter::default(),
    };

    tokio::spawn(api::review_job::sweep_pending(state.clone()));

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "listening");
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
