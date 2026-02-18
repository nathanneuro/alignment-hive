#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod config;
mod jupyter;
mod runpod;
mod server;
mod ssh;
mod state;

use clap::Parser;
use rmcp::ServiceExt;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "remote-kernels",
    about = "MCP server for cloud GPU instances with Jupyter kernels"
)]
struct Cli {
    /// Project directory (where remote-kernels.toml lives).
    #[arg(long, default_value = ".")]
    project_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "remote_kernels=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let project_dir = cli.project_dir.canonicalize().unwrap_or(cli.project_dir);

    // Load .env.local (then .env) from project directory if present.
    let _ = dotenvy::from_path(project_dir.join(".env.local"));
    let _ = dotenvy::from_path(project_dir.join(".env"));

    let api_key = std::env::var("RUNPOD_API_KEY").map_err(|_| {
        "RUNPOD_API_KEY environment variable not set. Get your API key from https://runpod.io/console/user/settings"
    })?;

    let config = config::Config::load(&project_dir)?;
    let app_state = state::AppState::new(project_dir);

    let server = server::RemoteKernelsServer::new(config, api_key, app_state);

    tracing::info!("Starting remote-kernels MCP server");

    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .inspect_err(|e| tracing::error!("Failed to start MCP server: {e}"))?;

    running.waiting().await?;

    Ok(())
}
