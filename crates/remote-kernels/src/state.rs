use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use tokio::sync::oneshot;

use crate::config::Cleanup;
use crate::heartbeat::HeartbeatState;
use crate::jupyter::messages::ExecutionOutput;
use crate::jupyter::rest::JupyterClient;
use crate::jupyter::ws::KernelConnection;
use crate::notebook::Notebook;
use crate::runpod::types::Pod;

/// Persisted state — written to `.claude/remote-kernels/state.json` so the stop hook can read it.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PersistedState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<String>,
    /// Accumulated session spend in dollars. Monotonically increasing, never resets.
    #[serde(default)]
    pub accumulated_spend: f64,
}

/// Runtime state held in memory by the MCP server.
pub struct AppState {
    pub project_dir: PathBuf,
    pub pod: Option<PodState>,
    /// Accumulated spend from previous pods in this session. Monotonically increasing.
    pub accumulated_spend: f64,
}

pub struct PodState {
    pub pod_id: String,
    pub gpu_name: String,
    pub cost_per_hr: f64,
    pub started_at: std::time::Instant,
    pub jupyter: JupyterClient,
    pub jupyter_token: String,
    pub session_id: String,
    pub kernel_ids: Vec<String>,
    pub kernel_connections: HashMap<String, KernelConnection>,
    pub notebooks: HashMap<String, Notebook>,
    pub ssh_key_path: PathBuf,
    pub public_ip: Option<String>,
    pub ssh_port: Option<u16>,
    pub heartbeat: Option<HeartbeatState>,
    /// Pending executions that timed out. Keyed by (`kernel_id`, `cell_number`).
    pub pending_executions: HashMap<(String, u32), oneshot::Receiver<ExecutionOutput>>,
}

impl PodState {
    /// Cost incurred by the current pod since it started.
    pub fn current_pod_cost(&self) -> f64 {
        self.cost_per_hr * self.started_at.elapsed().as_secs_f64() / 3600.0
    }
}

impl AppState {
    pub fn new(project_dir: PathBuf) -> Self {
        // Load accumulated spend from any previous state file.
        let accumulated_spend = Self::load_accumulated_spend(&project_dir);
        Self {
            project_dir,
            pod: None,
            accumulated_spend,
        }
    }

    fn state_dir(&self) -> PathBuf {
        self.project_dir.join(".claude/remote-kernels")
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir().join("state.json")
    }

    /// Total session spend: accumulated from previous pods + current pod's running cost.
    pub fn total_spend(&self) -> f64 {
        self.accumulated_spend + self.pod.as_ref().map_or(0.0, PodState::current_pod_cost)
    }

    /// Persist the current state to disk.
    pub fn save(&self, cleanup: Cleanup) -> anyhow::Result<()> {
        let dir = self.state_dir();
        std::fs::create_dir_all(&dir)?;

        // Ensure .gitignore exists so state files are never committed.
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            let _ = std::fs::write(&gitignore, "*\n");
        }

        let persisted = PersistedState {
            pod_id: self.pod.as_ref().map(|p| p.pod_id.clone()),
            cleanup: Some(match cleanup {
                Cleanup::Stop => "stop".to_string(),
                Cleanup::Terminate => "terminate".to_string(),
                Cleanup::Disabled => "disabled".to_string(),
            }),
            accumulated_spend: self.total_spend(),
        };

        let json = serde_json::to_string_pretty(&persisted)?;
        std::fs::write(self.state_path(), json)?;
        Ok(())
    }

    /// Snapshot accumulated spend (adds current pod cost to accumulated total).
    /// Called when a pod is stopped/terminated so the spend persists.
    pub fn snapshot_spend(&mut self) {
        if let Some(ref pod) = self.pod {
            self.accumulated_spend += pod.current_pod_cost();
        }
    }

    /// Clear persisted state (called after pod is stopped/terminated).
    pub fn clear(&self) -> anyhow::Result<()> {
        let path = self.state_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Record that a pod has started.
    pub fn set_pod(
        &mut self,
        pod: &Pod,
        jupyter: JupyterClient,
        jupyter_token: String,
        ssh_key_path: PathBuf,
    ) {
        self.pod = Some(PodState {
            pod_id: pod.id.clone(),
            gpu_name: pod.gpu_display_name().to_string(),
            cost_per_hr: pod.cost_per_hr.unwrap_or(0.0),
            started_at: std::time::Instant::now(),
            jupyter,
            jupyter_token,
            session_id: uuid::Uuid::new_v4().to_string(),
            kernel_ids: Vec::new(),
            kernel_connections: HashMap::new(),
            notebooks: HashMap::new(),
            ssh_key_path,
            public_ip: None,
            ssh_port: None,
            heartbeat: None,
            pending_executions: HashMap::new(),
        });
    }

    /// Load any existing state from disk (e.g. if the MCP server restarts while a pod is running).
    pub fn load_existing(project_dir: &Path) -> Option<String> {
        let path = project_dir.join(".claude/remote-kernels/state.json");
        let content = std::fs::read_to_string(path).ok()?;
        let state: PersistedState = serde_json::from_str(&content).ok()?;
        state.pod_id
    }

    /// Load accumulated spend from a previous state file.
    fn load_accumulated_spend(project_dir: &Path) -> f64 {
        let path = project_dir.join(".claude/remote-kernels/state.json");
        let Ok(content) = std::fs::read_to_string(path) else {
            return 0.0;
        };
        serde_json::from_str::<PersistedState>(&content)
            .map(|s| s.accumulated_spend)
            .unwrap_or(0.0)
    }
}
