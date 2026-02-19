use std::fmt::Write;
use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::descriptions;
use crate::jupyter::rest::JupyterClient;
use crate::runpod::client::RunPodClient;
use crate::runpod::types::PodCreateInput;
use crate::state::AppState;

#[derive(Clone)]
pub struct RemoteKernelsServer {
    config: Arc<Config>,
    runpod: Arc<RunPodClient>,
    state: Arc<Mutex<AppState>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

// --- Tool parameter types ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartParams {
    /// Override GPU type for this session.
    pub gpu_type: Option<String>,
    /// Override image for this session.
    pub image: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateKernelParams {
    /// Human-readable name for the kernel (used in notebook filename).
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KernelIdParams {
    /// The kernel ID to operate on.
    pub kernel_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteParams {
    /// The kernel ID to execute in.
    pub kernel_id: String,
    /// Python code to execute.
    pub code: String,
    /// Timeout in seconds (default: 30).
    pub timeout: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DownloadParams {
    /// Remote path on the pod to download.
    pub remote_path: String,
    /// Local path to save to.
    pub local_path: String,
}

fn generate_token() -> String {
    use std::fmt::Write as _;

    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

// --- Tool implementations ---

#[tool_router]
impl RemoteKernelsServer {
    pub fn new(config: Config, api_key: String, state: AppState) -> Self {
        Self {
            config: Arc::new(config),
            runpod: Arc::new(RunPodClient::new(api_key)),
            state: Arc::new(Mutex::new(state)),
            tool_router: Self::tool_router(),
        }
    }

    /// Spin up a GPU pod. Uses settings from remote-kernels.toml, with optional overrides.
    /// Returns pod info.
    #[tool(name = "start")]
    async fn start(&self, params: Parameters<StartParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;

        // Check if a pod is already running
        {
            let state = self.state.lock().await;
            if let Some(ref pod) = state.pod {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "A pod is already running (id: {}). Use status() to check it, or stop()/terminate() first.",
                    pod.pod_id
                ))]));
            }
        }

        // Generate ephemeral SSH keypair and Jupyter token.
        let project_dir = self.state.lock().await.project_dir.clone();
        let ssh_keypair = crate::ssh::generate_keypair(&project_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to generate SSH keypair: {e}"), None)
        })?;
        let jupyter_token = generate_token();

        let gpu_type_ids = if let Some(ref gpu) = params.gpu_type {
            vec![gpu.clone()]
        } else {
            self.config.gpu_type_ids.clone()
        };

        let image_name = params
            .image
            .unwrap_or_else(|| self.config.image_name.clone());

        // Merge user env with required env vars.
        let mut env = self.config.env.clone();
        env.insert("PUBLIC_KEY".to_string(), ssh_keypair.public_key_openssh);
        env.insert("JUPYTER_PASSWORD".to_string(), jupyter_token.clone());

        tracing::info!("Creating pod...");

        // Try each GPU type in order. For each GPU type:
        // - Availability errors (parsed from 500 body) → skip to next GPU type immediately
        // - Other 500 errors → retry up to 3 times with 1s delay, then move to next GPU type
        // - Non-500 errors → fail immediately
        let created_pod = {
            let mut failures: Vec<(String, String)> = Vec::new();
            let mut result_pod = None;

            for gpu_type in &gpu_type_ids {
                let input = PodCreateInput {
                    name: self.config.name.clone(),
                    image_name: image_name.clone(),
                    gpu_type_ids: vec![gpu_type.clone()],
                    gpu_count: Some(self.config.gpu_count),
                    cloud_type: Some(self.config.cloud_type.clone()),
                    container_disk_in_gb: Some(self.config.container_disk_gb),
                    volume_in_gb: if self.config.volume_gb > 0 {
                        Some(self.config.volume_gb)
                    } else {
                        None
                    },
                    volume_mount_path: Some(self.config.volume_mount_path.clone()),
                    network_volume_id: self.config.network_volume_id.clone(),
                    ports: Some(vec!["8888/http".to_string(), "22/tcp".to_string()]),
                    env: Some(env.clone()),
                    docker_start_cmd: Some(self.build_startup_cmd()),
                };

                tracing::info!(gpu_type = %gpu_type, "Trying GPU type...");

                let mut succeeded = false;
                for attempt in 1..=3 {
                    match self.runpod.create_pod(&input).await {
                        Ok(p) => {
                            result_pod = Some(p);
                            succeeded = true;
                            break;
                        }
                        Err(e) if e.is_availability_error() => {
                            tracing::info!(gpu_type = %gpu_type, error = %e, "No availability, skipping to next GPU type");
                            failures.push((gpu_type.clone(), format!("no availability: {e}")));
                            break;
                        }
                        Err(e) if e.is_server_error() && attempt < 3 => {
                            tracing::info!(gpu_type = %gpu_type, attempt, error = %e, "Server error, retrying...");
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        Err(e) if e.is_server_error() => {
                            // Final retry attempt failed
                            tracing::info!(gpu_type = %gpu_type, error = %e, "Server error on final attempt, moving to next GPU type");
                            failures.push((
                                gpu_type.clone(),
                                format!("server error after {attempt} attempts: {e}"),
                            ));
                            break;
                        }
                        Err(e) => {
                            // Non-server error (e.g. 4xx, network) — fail immediately
                            return Err(McpError::internal_error(
                                format!("Failed to create pod: {e}"),
                                None,
                            ));
                        }
                    }
                }

                if succeeded {
                    break;
                }
            }

            result_pod.ok_or_else(|| {
                let mut msg = String::from("Failed to create pod — all GPU types exhausted:\n");
                for (gpu, reason) in &failures {
                    let _ = writeln!(msg, "  - {gpu}: {reason}");
                }
                msg.push_str("\nConsider editing gpu-type-ids in remote-kernels.toml to try different GPU types.");
                McpError::internal_error(msg, None)
            })?
        };

        tracing::info!(pod_id = %created_pod.id, gpu = %created_pod.gpu_display_name(), "Pod created");

        let gpu_name = created_pod.gpu_display_name().to_string();
        let cost = created_pod.cost_per_hr.unwrap_or(0.0);
        let jupyter = JupyterClient::new(&created_pod.id, &jupyter_token);

        // Save state immediately so we can clean up even if polling fails.
        {
            let mut state = self.state.lock().await;
            state.set_pod(
                &created_pod,
                jupyter,
                jupyter_token.clone(),
                ssh_keypair.private_key_path,
            );
            if let Err(e) = state.save(self.config.cleanup) {
                tracing::warn!("Failed to persist state: {e}");
            }
        }

        // Wait for pod to be running.
        self.wait_for_running(&created_pod.id)
            .await
            .map_err(|e| McpError::internal_error(format!("Pod failed to start: {e}"), None))?;

        // Wait for SSH info (public IP + port mapping). These may lag behind
        // the RUNNING status by a few seconds in the RunPod API.
        let (ssh_ip, ssh_port) = match self.wait_for_ssh_info(&created_pod.id).await {
            Ok(info) => info,
            Err(e) => {
                // Pod is useless without SSH — terminate it.
                tracing::warn!("Pod has no SSH info, terminating: {e}");
                let _ = self.runpod.terminate_pod(&created_pod.id).await;
                let mut state = self.state.lock().await;
                state.pod.take();
                let _ = state.clear();
                return Err(McpError::internal_error(format!("{e}"), None));
            }
        };

        // Cache SSH info and start heartbeat in background (non-blocking).
        {
            let mut state = self.state.lock().await;
            if let Some(ref mut pod_state) = state.pod {
                pod_state.public_ip = Some(ssh_ip.clone());
                pod_state.ssh_port = Some(ssh_port);

                let hb = crate::heartbeat::start(
                    pod_state.ssh_key_path.clone(),
                    ssh_ip,
                    ssh_port,
                    self.config.cleanup,
                );
                pod_state.heartbeat = Some(hb);
            }
        }

        // Wait for Jupyter server to be ready.
        {
            let state = self.state.lock().await;
            let pod_state = state.pod.as_ref().expect("pod state exists");
            pod_state.jupyter.wait_until_ready().await.map_err(|e| {
                McpError::internal_error(format!("Jupyter failed to start: {e}"), None)
            })?;
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Pod started successfully!\n\
             - ID: {}\n\
             - GPU: {gpu_name}\n\
             - Cost: ${cost:.2}/hr\n\
             - Status: RUNNING\n\
             \n\
             Use create_kernel() to start a kernel.",
            created_pod.id,
        ))]))
    }

    /// Stop the running pod. The pod is preserved and can be restarted (storage costs still apply).
    /// Use terminate() to fully delete it.
    #[tool(name = "stop")]
    async fn stop(&self, _params: Parameters<EmptyParams>) -> Result<CallToolResult, McpError> {
        let pod_id = {
            let state = self.state.lock().await;
            match &state.pod {
                Some(pod) => pod.pod_id.clone(),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "No pod is running. Call start() first.",
                    )]));
                }
            }
        };

        tracing::info!(pod_id = %pod_id, "Stopping pod...");

        self.runpod
            .stop_pod(&pod_id)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to stop pod: {e}"), None))?;

        let mut state = self.state.lock().await;
        let elapsed = state
            .pod
            .as_ref()
            .map(|p| p.started_at.elapsed())
            .unwrap_or_default();
        let cost = state
            .pod
            .as_ref()
            .map_or(0.0, |p| p.cost_per_hr * elapsed.as_secs_f64() / 3600.0);
        if let Some(mut pod) = state.pod.take()
            && let Some(hb) = pod.heartbeat.take()
        {
            hb.stop();
        }
        if let Err(e) = state.clear() {
            tracing::warn!("Failed to clear state file: {e}");
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Pod {pod_id} stopped. Session cost: ${cost:.2}. The pod is preserved and can be restarted from the RunPod dashboard.",
        ))]))
    }

    /// Terminate (delete) the running pod. All data on the pod is lost.
    /// Network volumes are preserved.
    #[tool(name = "terminate")]
    async fn terminate(
        &self,
        _params: Parameters<EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let pod_id = {
            let state = self.state.lock().await;
            match &state.pod {
                Some(pod) => pod.pod_id.clone(),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "No pod is running. Call start() first.",
                    )]));
                }
            }
        };

        tracing::info!(pod_id = %pod_id, "Terminating pod...");

        self.runpod
            .terminate_pod(&pod_id)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to terminate pod: {e}"), None))?;

        let mut state = self.state.lock().await;
        let elapsed = state
            .pod
            .as_ref()
            .map(|p| p.started_at.elapsed())
            .unwrap_or_default();
        let cost = state
            .pod
            .as_ref()
            .map_or(0.0, |p| p.cost_per_hr * elapsed.as_secs_f64() / 3600.0);
        if let Some(mut pod) = state.pod.take()
            && let Some(hb) = pod.heartbeat.take()
        {
            hb.stop();
        }
        if let Err(e) = state.clear() {
            tracing::warn!("Failed to clear state file: {e}");
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Pod {pod_id} terminated. Session cost: ${cost:.2}. All pod data has been deleted.",
        ))]))
    }

    /// Get the current pod status including GPU, cost, and uptime.
    #[tool(name = "status")]
    async fn status(&self, _params: Parameters<EmptyParams>) -> Result<CallToolResult, McpError> {
        let pod_id = {
            let state = self.state.lock().await;
            match &state.pod {
                Some(pod) => pod.pod_id.clone(),
                None => {
                    return Ok(CallToolResult::success(vec![Content::text(
                        "No pod is currently running.",
                    )]));
                }
            }
        };

        let pod = self.runpod.get_pod(&pod_id).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get pod status: {e}"), None)
        })?;

        let state = self.state.lock().await;
        let pod_state = state.pod.as_ref().expect("pod state exists");
        let elapsed = pod_state.started_at.elapsed();
        let session_cost = pod_state.cost_per_hr * elapsed.as_secs_f64() / 3600.0;
        let cost_per_hr = pod_state.cost_per_hr;
        let uptime_mins = elapsed.as_secs() / 60;

        let mut info = format!(
            "Pod: {}\n\
             Status: {}\n\
             GPU: {}\n\
             Cost: ${cost_per_hr:.2}/hr\n\
             Uptime: {uptime_mins} minutes\n\
             Session cost: ${session_cost:.2}\n\
             Kernels: {}",
            pod.id,
            pod.desired_status.as_deref().unwrap_or("unknown"),
            pod_state.gpu_name,
            if pod_state.kernel_ids.is_empty() {
                "none".to_string()
            } else {
                pod_state.kernel_ids.join(", ")
            },
        );

        if let Some(cap) = self.config.budget_cap {
            let remaining = cap - session_cost;
            let _ = write!(
                info,
                "\nBudget: ${session_cost:.2} / ${cap:.2} (${remaining:.2} remaining)"
            );
        }

        Ok(CallToolResult::success(vec![Content::text(info)]))
    }

    /// Spin up an additional kernel on the running pod. Returns the new kernel ID.
    #[tool(name = "create_kernel")]
    async fn create_kernel(
        &self,
        params: Parameters<CreateKernelParams>,
    ) -> Result<CallToolResult, McpError> {
        let name = params.0.name;

        let state = self.state.lock().await;
        let Some(pod_state) = &state.pod else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No pod is running. Call start() first.",
            )]));
        };

        let kernel =
            pod_state.jupyter.create_kernel().await.map_err(|e| {
                McpError::internal_error(format!("Failed to create kernel: {e}"), None)
            })?;

        let kernel_id = kernel.id;
        let pod_id = pod_state.pod_id.clone();
        let token = pod_state.jupyter_token.clone();
        drop(state);

        let conn = crate::jupyter::ws::KernelConnection::connect(&pod_id, &kernel_id, &token)
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Failed to connect WebSocket to kernel: {e}"),
                    None,
                )
            })?;

        {
            let mut state = self.state.lock().await;
            let project_dir = state.project_dir.clone();
            if let Some(ref mut pod) = state.pod {
                pod.kernel_ids.push(kernel_id.clone());
                pod.kernel_connections.insert(kernel_id.clone(), conn);

                if let Ok(nb) =
                    crate::notebook::Notebook::new(&project_dir, &kernel_id, name.as_deref())
                {
                    pod.notebooks.insert(kernel_id.clone(), nb);
                }
            }
        }

        let label = match &name {
            Some(n) => format!("{kernel_id} ({n})"),
            None => kernel_id.clone(),
        };
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Kernel created: {label}"
        ))]))
    }

    /// Execute Python code in a Jupyter kernel. Returns the output (stdout, stderr, result, errors).
    /// For long-running code, consider using a reasonable timeout.
    #[tool(name = "execute")]
    async fn execute(&self, params: Parameters<ExecuteParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;
        let timeout_secs = params.timeout.unwrap_or(30);
        let timeout = std::time::Duration::from_secs(timeout_secs);

        let mut state = self.state.lock().await;
        let Some(pod_state) = &mut state.pod else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No pod is running. Call start() first.",
            )]));
        };

        let Some(conn) = pod_state.kernel_connections.get(&params.kernel_id) else {
            let available: Vec<_> = pod_state.kernel_ids.clone();
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Kernel {} not found. Available kernels: {}",
                params.kernel_id,
                if available.is_empty() {
                    "none".to_string()
                } else {
                    available.join(", ")
                }
            ))]));
        };

        let output = conn
            .execute(&pod_state.session_id, &params.code, timeout)
            .await
            .map_err(|e| McpError::internal_error(format!("Execution failed: {e}"), None))?;

        // Auto-save to notebook.
        if let Some(nb) = pod_state.notebooks.get_mut(&params.kernel_id)
            && let Err(e) = nb.append_cell(&params.code, &output)
        {
            tracing::warn!("Failed to save notebook cell: {e}");
        }

        let formatted = output.format();
        let is_error = output.error.is_some();

        if is_error {
            Ok(CallToolResult::error(vec![Content::text(formatted)]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(formatted)]))
        }
    }

    /// Sync local project files to the running pod via rsync over SSH. Respects .gitignore.
    /// Requires the pod to have a public IP (may not work on all community cloud machines).
    #[tool(name = "sync")]
    async fn sync(&self, _params: Parameters<EmptyParams>) -> Result<CallToolResult, McpError> {
        let (project_dir, ssh_key_path) = {
            let state = self.state.lock().await;
            let Some(pod_state) = &state.pod else {
                return Ok(CallToolResult::error(vec![Content::text(
                    "No pod is running. Call start() first.",
                )]));
            };
            (state.project_dir.clone(), pod_state.ssh_key_path.clone())
        };

        let (public_ip, ssh_port) = self.get_ssh_info().await?;
        let volume_mount = self.config.volume_mount_path.clone();

        let result = crate::sync::sync_to_pod(
            &project_dir,
            &ssh_key_path,
            &public_ip,
            ssh_port,
            &volume_mount,
        )
        .await
        .map_err(|e| McpError::internal_error(format!("Sync failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Download a file or directory from the pod to a local path.
    #[tool(name = "download")]
    async fn download(
        &self,
        params: Parameters<DownloadParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = params.0;

        let ssh_key_path = {
            let state = self.state.lock().await;
            let Some(pod_state) = &state.pod else {
                return Ok(CallToolResult::error(vec![Content::text(
                    "No pod is running. Call start() first.",
                )]));
            };
            pod_state.ssh_key_path.clone()
        };

        let (public_ip, ssh_port) = self.get_ssh_info().await?;

        let result = crate::sync::download_from_pod(
            &ssh_key_path,
            &public_ip,
            ssh_port,
            &params.remote_path,
            std::path::Path::new(&params.local_path),
        )
        .await
        .map_err(|e| McpError::internal_error(format!("Download failed: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Shut down a kernel and free its resources.
    #[tool(name = "shutdown_kernel")]
    async fn shutdown_kernel(
        &self,
        params: Parameters<KernelIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let kernel_id = &params.0.kernel_id;

        let state = self.state.lock().await;
        let Some(pod_state) = &state.pod else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No pod is running. Call start() first.",
            )]));
        };

        pod_state
            .jupyter
            .delete_kernel(kernel_id)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to shut down kernel: {e}"), None)
            })?;

        let kernel_id = kernel_id.clone();
        drop(state);

        {
            let mut state = self.state.lock().await;
            if let Some(ref mut pod) = state.pod {
                pod.kernel_ids.retain(|id| *id != kernel_id);
                pod.kernel_connections.remove(&kernel_id);
            }
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Kernel {kernel_id} shut down."
        ))]))
    }

    /// Interrupt the currently running execution in a kernel.
    #[tool(name = "interrupt")]
    async fn interrupt(
        &self,
        params: Parameters<KernelIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let kernel_id = &params.0.kernel_id;

        let state = self.state.lock().await;
        let Some(pod_state) = &state.pod else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No pod is running. Call start() first.",
            )]));
        };

        pod_state
            .jupyter
            .interrupt_kernel(kernel_id)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to interrupt kernel: {e}"), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Kernel {kernel_id} interrupted."
        ))]))
    }

    /// Restart a kernel (clears all state but preserves the kernel ID).
    #[tool(name = "restart_kernel")]
    async fn restart_kernel(
        &self,
        params: Parameters<KernelIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let kernel_id = &params.0.kernel_id;

        let state = self.state.lock().await;
        let Some(pod_state) = &state.pod else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No pod is running. Call start() first.",
            )]));
        };

        pod_state
            .jupyter
            .restart_kernel(kernel_id)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to restart kernel: {e}"), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Kernel {kernel_id} restarted."
        ))]))
    }
}

impl RemoteKernelsServer {
    /// Get a clone of the shared state for use outside the MCP server (e.g. graceful shutdown).
    pub fn shared_state(&self) -> Arc<Mutex<AppState>> {
        Arc::clone(&self.state)
    }

    /// Build the startup command for the pod.
    /// Installs rsync (not in default RunPod images) and runs user startup commands.
    fn build_startup_cmd(&self) -> Vec<String> {
        let mut parts = vec!["apt-get update -qq && apt-get install -y -qq rsync".to_string()];
        parts.extend(self.config.startup_commands.clone());
        // RunPod dockerStartCmd runs each element as a separate shell command.
        // Combine into a single command so ordering is guaranteed.
        vec![parts.join(" && ")]
    }

    /// Poll until the pod reaches RUNNING status.
    async fn wait_for_running(&self, pod_id: &str) -> anyhow::Result<crate::runpod::types::Pod> {
        let mut attempts = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            attempts += 1;

            let pod = self.runpod.get_pod(pod_id).await?;
            tracing::debug!(
                pod_id,
                status = ?pod.desired_status,
                attempt = attempts,
                "Polling pod status"
            );

            if pod.is_running() {
                return Ok(pod);
            }

            if attempts > 60 {
                anyhow::bail!(
                    "Pod did not reach RUNNING status after 3 minutes (current: {:?})",
                    pod.desired_status
                );
            }
        }
    }

    /// Poll the GraphQL API until the pod has SSH connection info.
    ///
    /// Runtime port mappings may lag behind the RUNNING status by a few seconds.
    async fn wait_for_ssh_info(&self, pod_id: &str) -> anyhow::Result<(String, u16)> {
        for attempt in 1..=20 {
            match self.runpod.get_ssh_info(pod_id).await {
                Ok(Some((ip, port))) => {
                    tracing::info!(attempt, %ip, port, "SSH info available");
                    return Ok((ip, port));
                }
                Ok(None) => {
                    tracing::debug!(attempt, "SSH info not yet available");
                }
                Err(e) => {
                    tracing::debug!(attempt, error = %e, "Failed to query SSH info");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }

        anyhow::bail!(
            "Pod does not have a public IP or SSH port after 60s. \
             This is required for heartbeat, sync, and download. \
             Try starting again — a different machine may be assigned."
        )
    }

    /// Get SSH connection info from cached state. Refreshes from API if not cached.
    async fn get_ssh_info(&self) -> Result<(String, u16), McpError> {
        {
            let state = self.state.lock().await;
            if let Some(ref pod) = state.pod
                && let (Some(ip), Some(port)) = (&pod.public_ip, pod.ssh_port)
            {
                return Ok((ip.clone(), port));
            }
        }

        // Not cached — this shouldn't happen after start() succeeds, but handle it.
        let pod_id = {
            let state = self.state.lock().await;
            state
                .pod
                .as_ref()
                .map(|p| p.pod_id.clone())
                .ok_or_else(|| McpError::internal_error("No pod running", None))?
        };

        let (ip, port) = self.wait_for_ssh_info(&pod_id).await.map_err(|e| {
            McpError::internal_error(format!("Failed to get SSH info: {e}"), None)
        })?;

        let mut state = self.state.lock().await;
        if let Some(ref mut pod_state) = state.pod {
            pod_state.public_ip = Some(ip.clone());
            pod_state.ssh_port = Some(port);
        }

        Ok((ip, port))
    }
}

#[tool_handler]
impl ServerHandler for RemoteKernelsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(descriptions::SERVER_INSTRUCTIONS.to_string()),
            ..Default::default()
        }
    }
}
