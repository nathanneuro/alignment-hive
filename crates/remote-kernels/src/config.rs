use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// GPU types to try, in order of preference.
    #[serde(default = "default_gpu_type_ids")]
    pub gpu_type_ids: Vec<String>,

    /// Container image to run on the pod.
    #[serde(default = "default_image_name")]
    pub image_name: String,

    /// What to do when cleaning up: "stop" preserves the pod, "terminate" deletes it,
    /// "disabled" skips automatic cleanup entirely.
    #[serde(default = "default_cleanup")]
    pub cleanup: Cleanup,

    /// Custom name prefix for pods.
    #[serde(default = "default_name")]
    pub name: String,

    /// Per-session budget cap in dollars.
    pub budget_cap: Option<f64>,

    /// Environment variable names to forward from the local environment to the pod.
    #[serde(default)]
    pub inherit_env: Vec<String>,

    /// Path to a dotenv file whose variables should be loaded onto the pod.
    pub env_file: Option<PathBuf>,

    /// Extra environment variables to set on the pod.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Directory for notebook files. Defaults to `remote-kernels/` at project root.
    #[serde(default = "default_notebook_dir")]
    pub notebook_dir: PathBuf,

    /// Extra paths to include when syncing, even if gitignored.
    #[serde(default)]
    pub sync_include: Vec<String>,

    /// Commands to run in the pod startup script (after services start).
    #[serde(default)]
    pub startup_commands: Vec<String>,

    /// `RunPod` API passthrough fields. Typed fields are handled directly;
    /// any extra fields are passed through to the pod creation API as-is.
    #[serde(default)]
    pub runpod: RunpodConfig,
}

/// RunPod-specific configuration. Known fields are typed; unknown fields are passed
/// through transparently to the `RunPod` pod creation API (camelCase conversion applied).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunpodConfig {
    /// Number of GPUs to attach.
    #[serde(default = "default_gpu_count")]
    pub gpu_count: u32,

    /// Container disk size in GB.
    #[serde(default = "default_container_disk_gb")]
    pub container_disk_gb: u32,

    /// Persistent volume size in GB. Set to 0 to disable.
    #[serde(default = "default_volume_gb")]
    pub volume_gb: u32,

    /// Mount path for volumes.
    #[serde(default = "default_volume_mount_path")]
    pub volume_mount_path: String,

    /// Network volume ID to attach (optional).
    pub network_volume_id: Option<String>,

    /// Cloud type: "SECURE" or "COMMUNITY".
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,

    /// Extra fields passed through to the `RunPod` API.
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cleanup {
    /// Stop the pod (preserves state, can be restarted, still incurs storage costs).
    #[default]
    Stop,
    /// Terminate/delete the pod (all data lost).
    Terminate,
    /// Disabled: no automatic cleanup. User must stop/terminate manually.
    Disabled,
}

fn default_gpu_type_ids() -> Vec<String> {
    vec!["NVIDIA GeForce RTX 4090".to_string()]
}

fn default_gpu_count() -> u32 {
    1
}

fn default_image_name() -> String {
    "runpod/pytorch:2.1.0-py3.10-cuda11.8.0-devel-ubuntu22.04".to_string()
}

fn default_container_disk_gb() -> u32 {
    50
}

fn default_volume_gb() -> u32 {
    20
}

fn default_volume_mount_path() -> String {
    "/workspace".to_string()
}

fn default_cleanup() -> Cleanup {
    Cleanup::Terminate
}

fn default_cloud_type() -> String {
    "SECURE".to_string()
}

fn default_name() -> String {
    "remote-kernels".to_string()
}

fn default_notebook_dir() -> PathBuf {
    PathBuf::from("remote-kernels")
}

impl Config {
    pub fn load(project_dir: &Path) -> anyhow::Result<Self> {
        let config_path = project_dir.join("remote-kernels.toml");
        if !config_path.exists() {
            tracing::info!("No remote-kernels.toml found, using defaults");
            return Ok(toml::from_str("")?);
        }
        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = toml::from_str(&content)?;
        tracing::info!(?config_path, "Loaded config");
        Ok(config)
    }
}
