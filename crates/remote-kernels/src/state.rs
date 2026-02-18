use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Cleanup;
use crate::heartbeat::HeartbeatState;
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
}

/// Runtime state held in memory by the MCP server.
pub struct AppState {
    pub project_dir: PathBuf,
    pub pod: Option<PodState>,
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
}

impl AppState {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            project_dir,
            pod: None,
        }
    }

    fn state_dir(&self) -> PathBuf {
        self.project_dir.join(".claude/remote-kernels")
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir().join("state.json")
    }

    /// Persist the current pod ID to disk so the stop hook can find it.
    pub fn save(&self, cleanup: Cleanup) -> anyhow::Result<()> {
        let dir = self.state_dir();
        std::fs::create_dir_all(&dir)?;

        let persisted = PersistedState {
            pod_id: self.pod.as_ref().map(|p| p.pod_id.clone()),
            cleanup: Some(match cleanup {
                Cleanup::Stop => "stop".to_string(),
                Cleanup::Terminate => "terminate".to_string(),
            }),
        };

        let json = serde_json::to_string_pretty(&persisted)?;
        std::fs::write(self.state_path(), json)?;
        Ok(())
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
        });
    }

    /// Load any existing state from disk (e.g. if the MCP server restarts while a pod is running).
    pub fn load_existing(project_dir: &Path) -> Option<String> {
        let path = project_dir.join(".claude/remote-kernels/state.json");
        let content = std::fs::read_to_string(path).ok()?;
        let state: PersistedState = serde_json::from_str(&content).ok()?;
        state.pod_id
    }
}
