use clap::Parser;
use rmcp::ServiceExt;
use std::path::PathBuf;

use remote_kernels::{config, runpod, server, state};

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
    let cleanup = config.cleanup;

    // Budget: env var overrides config. Env var is typically set via .claude/settings.json
    // so Claude can't modify it.
    let budget = std::env::var("REMOTE_KERNELS_BUDGET")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .or(config.budget_cap);

    // Budget and cleanup:disabled are incompatible — disabled means the user wants the
    // pod to keep running, which conflicts with budget enforcement stopping/terminating it.
    if budget.is_some() && cleanup == config::Cleanup::Disabled {
        return Err(
            "Configuration error: budget-cap (or REMOTE_KERNELS_BUDGET) cannot be used with cleanup = \"disabled\". \
             Budget enforcement requires the ability to stop/terminate the pod.".into()
        );
    }

    let app_state = state::AppState::new(project_dir);

    let server = server::RemoteKernelsServer::new(config, api_key.clone(), app_state, budget);
    let shared_state = server.shared_state();

    tracing::info!("Starting remote-kernels MCP server");

    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .inspect_err(|e| tracing::error!("Failed to start MCP server: {e}"))?;

    running.waiting().await?;

    // Graceful shutdown: clean up pod if one is running.
    tracing::info!("MCP server disconnected, cleaning up...");

    let mut state = shared_state.lock().await;
    if let Some(mut pod) = state.pod.take() {
        // Stop heartbeat.
        if let Some(hb) = pod.heartbeat.take() {
            hb.stop();
        }

        // Stop or terminate the pod based on config.
        match cleanup {
            config::Cleanup::Disabled => {
                tracing::info!(pod_id = %pod.pod_id, "Cleanup disabled, leaving pod running");
            }
            _ => {
                let runpod = runpod::client::RunPodClient::new(api_key);
                let result = match cleanup {
                    config::Cleanup::Stop => runpod.stop_pod(&pod.pod_id).await,
                    config::Cleanup::Terminate => runpod.terminate_pod(&pod.pod_id).await,
                    config::Cleanup::Disabled => unreachable!(),
                };
                match result {
                    Ok(()) => tracing::info!(pod_id = %pod.pod_id, ?cleanup, "Pod cleaned up"),
                    Err(e) => {
                        tracing::warn!(pod_id = %pod.pod_id, "Failed to clean up pod: {e}");
                    }
                }
            }
        }

        if let Err(e) = state.clear() {
            tracing::warn!("Failed to clear state file: {e}");
        }
    }

    Ok(())
}
