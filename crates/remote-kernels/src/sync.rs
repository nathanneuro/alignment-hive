use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// rsync local project files to the pod.
///
/// Uses the ephemeral SSH key generated at pod creation.
/// Respects `.gitignore` via rsync's `--filter=':- .gitignore'`.
pub async fn sync_to_pod(
    project_dir: &Path,
    ssh_key_path: &Path,
    public_ip: &str,
    ssh_port: u16,
    remote_path: &str,
) -> anyhow::Result<String> {
    let ssh_cmd = format!(
        "ssh -i {} -p {ssh_port} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
        ssh_key_path.display()
    );

    let source = format!("{}/", project_dir.display());
    let destination = format!("root@{public_ip}:{remote_path}/");

    tracing::info!(%destination, "Syncing files to pod");

    let output = Command::new("rsync")
        .args([
            "-az",
            "--delete",
            "--filter=:- .gitignore",
            "--exclude=.git",
            "--exclude=.claude",
            "--exclude=target",
            "--exclude=node_modules",
            "-e",
            &ssh_cmd,
            &source,
            &destination,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("rsync failed: {stderr}");
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        tracing::debug!(%stderr, "rsync stderr");
    }

    Ok("Files synced successfully.".to_string())
}

/// Download a file or directory from the pod to a local path.
pub async fn download_from_pod(
    ssh_key_path: &Path,
    public_ip: &str,
    ssh_port: u16,
    remote_path: &str,
    local_path: &Path,
) -> anyhow::Result<String> {
    let ssh_cmd = format!(
        "ssh -i {} -p {ssh_port} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
        ssh_key_path.display()
    );

    let source = format!("root@{public_ip}:{remote_path}");
    let destination = local_path.display().to_string();

    // Ensure parent directory exists.
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    tracing::info!(%source, %destination, "Downloading from pod");

    let output = Command::new("rsync")
        .args(["-az", "-e", &ssh_cmd, &source, &destination])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("rsync failed: {stderr}");
    }

    Ok(format!("Downloaded to {destination}"))
}
