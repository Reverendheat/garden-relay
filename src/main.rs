mod api;
mod config;
mod domain;
mod lifecycle;
mod policy;
mod provider;
mod state;
mod storage;

use tokio::net::TcpListener;

use crate::{config::Config, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gardenrelay=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let state = AppState::from_config(&config)?;
    let bind_addr = config.bind_addr()?;
    let listener = TcpListener::bind(bind_addr).await?;

    tracing::info!(%bind_addr, "garden relay listening");
    axum::serve(listener, api::router(state)).await?;

    Ok(())
}
