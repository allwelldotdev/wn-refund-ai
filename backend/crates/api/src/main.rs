//! `refund-api`         migrate, seed, then serve on BIND_ADDR (default 0.0.0.0:8080)
//! `refund-api seed`    migrate and seed, then exit

use std::net::SocketAddr;

use anyhow::Context;
use api::{AppState, build_router, prepare_database};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db = db::connect(&database_url)
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

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .context("BIND_ADDR must be host:port")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, build_router(AppState { db }))
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
