// Application entry point and startup wiring.

mod app;
mod config;
mod domain;
mod tg;
mod ui;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config =
        config::Config::load("config.toml").context("failed to load application config")?;
    let data_dir = config
        .data_dir()
        .context("failed to resolve application data directory")?;
    tracing::info!(data_dir = %data_dir.display(), "configuration loaded");

    let credentials = config
        .api_credentials()
        .context("invalid Telegram API credentials")?;
    let backend = tg::Adapter::new(config.use_test_dc(), credentials, data_dir)
        .context("failed to start Telegram backend")?;

    ui::run(backend)?;
    Ok(())
}
