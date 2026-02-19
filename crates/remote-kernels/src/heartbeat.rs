use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use tokio::process::Command;

use crate::config::Cleanup;
use crate::runpod::client::RunPodClient;

/// Start the heartbeat system in the background.
///
/// Spawns a background task that:
/// 1. Resolves SSH connection info via the RunPod GraphQL API
/// 2. Waits for SSH to become reachable on the pod
/// 3. Injects a watchdog process that checks `/tmp/heartbeat` every 30s
///    and cleans up the pod if stale (>5 min)
/// 4. Periodically touches `/tmp/heartbeat` via SSH to keep the watchdog alive
///
/// Returns immediately without blocking. If SSH info never becomes available
/// (e.g. the machine has no public IP), the task logs a warning and exits.
pub fn start(
    runpod: Arc<RunPodClient>,
    pod_id: String,
    ssh_key_path: PathBuf,
    cleanup: Cleanup,
) -> HeartbeatState {
    let handle = tokio::spawn(async move {
        if let Err(e) = run(&runpod, &pod_id, &ssh_key_path, cleanup).await {
            tracing::warn!("Heartbeat task failed: {e}");
        }
    });

    HeartbeatState {
        task_handle: handle,
    }
}

/// The main heartbeat pipeline: resolve SSH info, wait for SSH, inject watchdog,
/// then send heartbeats.
async fn run(
    runpod: &RunPodClient,
    pod_id: &str,
    ssh_key_path: &Path,
    cleanup: Cleanup,
) -> anyhow::Result<()> {
    let (ip, port) = resolve_ssh_info(runpod, pod_id).await?;
    wait_for_ssh(ssh_key_path, &ip, port).await?;
    inject_watchdog(ssh_key_path, &ip, port, cleanup).await?;

    tracing::info!("Heartbeat: watchdog running, starting heartbeat loop");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        match ssh_cmd(ssh_key_path, &ip, port, "touch /tmp/heartbeat").await {
            Ok(_) => tracing::debug!("Heartbeat sent"),
            Err(e) => tracing::warn!("Heartbeat failed: {e}"),
        }
    }
}

/// Poll the GraphQL API until SSH connection info is available.
async fn resolve_ssh_info(runpod: &RunPodClient, pod_id: &str) -> anyhow::Result<(String, u16)> {
    for attempt in 1..=40 {
        match runpod.get_ssh_info(pod_id).await {
            Ok(Some((ip, port))) => {
                tracing::info!(attempt, %ip, port, "Heartbeat: SSH info resolved");
                return Ok((ip, port));
            }
            Ok(None) => {
                tracing::debug!(attempt, "Heartbeat: SSH info not yet available");
            }
            Err(e) => {
                tracing::debug!(attempt, error = %e, "Heartbeat: failed to query SSH info");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    anyhow::bail!(
        "SSH info not available after 2 minutes — heartbeat not started. \
         sync() and download() may still work (they resolve SSH info on demand)."
    )
}

/// Wait for SSH to become reachable, retrying up to ~2 minutes.
async fn wait_for_ssh(ssh_key_path: &Path, public_ip: &str, ssh_port: u16) -> anyhow::Result<()> {
    for attempt in 1..=24 {
        match ssh_cmd(ssh_key_path, public_ip, ssh_port, "echo ok").await {
            Ok(_) => {
                tracing::info!(attempt, "Heartbeat: SSH is reachable");
                return Ok(());
            }
            Err(e) => {
                tracing::debug!(attempt, error = %e, "Heartbeat: SSH not ready yet");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    anyhow::bail!("SSH did not become reachable after 2 minutes")
}

/// Inject the watchdog process on the pod via SSH.
///
/// The watchdog runs as a detached background process that monitors `/tmp/heartbeat`.
/// If the file is stale for >5 minutes, it cleans up the pod to prevent runaway costs.
async fn inject_watchdog(
    ssh_key_path: &Path,
    public_ip: &str,
    ssh_port: u16,
    cleanup: Cleanup,
) -> anyhow::Result<()> {
    let runpodctl_cmd = match cleanup {
        Cleanup::Stop => "runpodctl stop pod $RUNPOD_POD_ID",
        Cleanup::Terminate => "runpodctl remove pod $RUNPOD_POD_ID",
    };

    // The script is wrapped in single quotes for bash -c, so $ expansions happen
    // inside the inner bash (which inherits the pod's environment including RUNPOD_POD_ID).
    // {{age}} is Rust format escaping that produces ${age} in the output.
    let watchdog = format!(
        concat!(
            "nohup bash -c '",
            "touch /tmp/heartbeat; ",
            "while true; do ",
            "sleep 30; ",
            "age=$(($(date +%s) - $(stat -c %Y /tmp/heartbeat 2>/dev/null || echo 0))); ",
            r#"if [ "$age" -gt 300 ]; then "#,
            r#"echo "Heartbeat stale (${{age}}s), cleaning up pod..." >> /tmp/watchdog.log; "#,
            "{cmd}; ",
            "exit 0; ",
            "fi; ",
            "done' </dev/null >/dev/null 2>&1 &",
        ),
        cmd = runpodctl_cmd
    );

    ssh_cmd(ssh_key_path, public_ip, ssh_port, &watchdog).await?;
    tracing::info!("Heartbeat: watchdog injected on pod");
    Ok(())
}

/// Execute a command on the pod via SSH.
async fn ssh_cmd(
    ssh_key_path: &Path,
    public_ip: &str,
    ssh_port: u16,
    command: &str,
) -> anyhow::Result<String> {
    let key_path = ssh_key_path.display().to_string();
    let port = ssh_port.to_string();
    let host = format!("root@{public_ip}");

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new("ssh")
            .args([
                "-i",
                &key_path,
                "-p",
                &port,
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
                "-o",
                "ConnectTimeout=5",
                &host,
                command,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("SSH command timed out"))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("SSH command failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Cleanup state for graceful shutdown.
pub struct HeartbeatState {
    pub task_handle: tokio::task::JoinHandle<()>,
}

impl HeartbeatState {
    pub fn stop(self) {
        self.task_handle.abort();
    }
}
