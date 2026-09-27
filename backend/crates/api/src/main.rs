//! `refund-api`         migrate, seed, then serve on BIND_ADDR (default 0.0.0.0:8080)
//! `refund-api seed`    migrate and seed, then exit

use std::sync::Arc;

use ai::OpenRouterAssistant;
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

    let serve = match std::env::args().nth(1).as_deref() {
        None => true,
        Some("seed") => false,
        Some(other) => anyhow::bail!("unknown command `{other}`; expected no argument or `seed`"),
    };
    let config = Config::from_env()?;
    // Checked before touching the database, so a missing key fails fast.
    let assistant = if serve {
        let key = config.openrouter_api_key()?;
        Some(OpenRouterAssistant::new(key).context("building the OpenRouter client")?)
    } else {
        None
    };

    let db = db::connect(&config.database_url)
        .await
        .context("connecting to Postgres")?;
    let report = prepare_database(&db)
        .await
        .context("migrating and seeding")?;
    tracing::info!(?report, "database ready");

    let Some(assistant) = assistant else {
        return Ok(());
    };
    let ai = &config.ai;
    tracing::info!(
        intake = %ai.intake.model,
        responder = %ai.responder.model,
        review = %ai.review.model,
        notice = %ai.notice.model,
        fallback = %ai.fallback_model,
        "LLM provider: OpenRouter"
    );
    let state = AppState {
        db,
        assistant: Arc::new(assistant),
        ai: Arc::new(config.ai),
        rate: RateLimiter::default(),
        login_throttle: Default::default(),
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
