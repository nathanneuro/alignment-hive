use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// GPU types to try, in order of preference.
    #[serde(default = "default_gpu_type_ids")]
    pub gpu_type_ids: Vec<String>,

    /// Number of GPUs to attach.
    #[serde(default = "default_gpu_count")]
    pub gpu_count: u32,

    /// Container image to run on the pod.
    #[serde(default = "default_image_name")]
    pub image_name: String,

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

    /// What to do when cleaning up: "stop" preserves the pod, "terminate" deletes it.
    #[serde(default = "default_cleanup")]
    pub cleanup: Cleanup,

    /// Cloud type: "secure" or "community".
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,

    /// Custom name prefix for pods.
    #[serde(default = "default_name")]
    pub name: String,

    /// Per-session budget cap in dollars.
    pub budget_cap: Option<f64>,

    /// Extra environment variables to set on the pod.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Commands to run in the pod startup script (after services start).
    #[serde(default)]
    pub startup_commands: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cleanup {
    /// Stop the pod (preserves state, can be restarted, still incurs storage costs).
    #[default]
    Stop,
    /// Terminate/delete the pod (all data lost).
    Terminate,
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
    Cleanup::Stop
}

fn default_cloud_type() -> String {
    "SECURE".to_string()
}

fn default_name() -> String {
    "remote-kernels".to_string()
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
