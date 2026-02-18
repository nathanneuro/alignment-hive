use std::sync::Arc;

use crate::config::Cleanup;
use crate::jupyter::rest::JupyterClient;
use crate::jupyter::ws::KernelConnection;

/// Start the watchdog process on the pod and begin sending heartbeats.
///
/// The watchdog is a background bash process that checks `/tmp/heartbeat` every 30s.
/// If the file is stale (>5 min), it cleans up the pod to prevent runaway costs.
/// The cleanup action (`stop` or `remove`) respects the user's config preference.
///
/// Returns the heartbeat kernel ID and a handle to the background heartbeat task.
pub async fn start(
    jupyter: &JupyterClient,
    pod_id: &str,
    token: &str,
    session_id: &str,
    cleanup: Cleanup,
) -> anyhow::Result<(String, tokio::task::JoinHandle<()>)> {
    // Create a dedicated kernel for heartbeat (separate from user kernels).
    let kernel = jupyter.create_kernel().await?;
    let kernel_id = kernel.id;

    let conn = KernelConnection::connect(pod_id, &kernel_id, token).await?;

    // Start the watchdog on the pod.
    let runpodctl_cmd = match cleanup {
        Cleanup::Stop => "runpodctl stop pod $RUNPOD_POD_ID",
        Cleanup::Terminate => "runpodctl remove pod $RUNPOD_POD_ID",
    };

    let watchdog_code = format!(
        r#"
import subprocess, os
subprocess.Popen(
    ['bash', '-c',
     'touch /tmp/heartbeat; '
     'while true; do '
     '  sleep 30; '
     '  age=$(($(date +%s) - $(stat -c %Y /tmp/heartbeat 2>/dev/null || echo 0))); '
     '  if [ "$age" -gt 300 ]; then '
     '    echo "Heartbeat stale (${{age}}s), cleaning up pod..." >> /tmp/watchdog.log; '
     '    {runpodctl_cmd}; '
     '    exit 0; '
     '  fi; '
     'done'],
    start_new_session=True,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
print("Watchdog started")
"#
    );

    let timeout = std::time::Duration::from_secs(10);
    let output = conn.execute(session_id, &watchdog_code, timeout).await?;
    tracing::info!(output = %output.format(), "Watchdog started on pod");

    // Spawn background heartbeat task.
    let session_id = session_id.to_string();
    let conn = Arc::new(conn);
    let conn_clone = Arc::clone(&conn);

    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let timeout = std::time::Duration::from_secs(10);
            match conn_clone
                .execute(
                    &session_id,
                    "import pathlib; pathlib.Path('/tmp/heartbeat').touch()",
                    timeout,
                )
                .await
            {
                Ok(_) => {
                    tracing::debug!("Heartbeat sent");
                }
                Err(e) => {
                    tracing::warn!("Heartbeat failed: {e}");
                }
            }
        }
    });

    Ok((kernel_id, handle))
}

/// Cleanup state for graceful shutdown.
pub struct HeartbeatState {
    pub kernel_id: String,
    pub task_handle: tokio::task::JoinHandle<()>,
}

impl HeartbeatState {
    pub fn stop(self) {
        self.task_handle.abort();
    }
}
