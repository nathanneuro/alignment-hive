use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Input for creating a new pod via `POST /pods`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodCreateInput {
    pub name: String,
    pub image_name: String,
    pub gpu_type_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_disk_in_gb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_in_gb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_mount_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_volume_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker_start_cmd: Option<Vec<String>>,
}

/// Pod response from the `RunPod` API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub desired_status: Option<String>,
    #[serde(default)]
    pub image_name: Option<String>,
    #[serde(default)]
    pub cost_per_hr: Option<f64>,
    #[serde(default)]
    pub gpu_count: Option<u32>,
    #[serde(default)]
    pub vcpu_count: Option<f64>,
    #[serde(default)]
    pub memory_in_gb: Option<f64>,
    #[serde(default)]
    pub last_started_at: Option<String>,
    #[serde(default)]
    pub public_ip: Option<String>,
    #[serde(default)]
    pub ports: Option<Vec<String>>,
    #[serde(default)]
    pub port_mappings: Option<HashMap<String, u16>>,
    #[serde(default)]
    pub machine_id: Option<String>,
    #[serde(default)]
    pub machine: Option<MachineInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineInfo {
    #[serde(default)]
    pub gpu_type_id: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

impl Pod {
    pub fn is_running(&self) -> bool {
        self.desired_status.as_deref() == Some("RUNNING")
    }

    /// Get the GPU type name from the machine info.
    pub fn gpu_display_name(&self) -> &str {
        self.machine
            .as_ref()
            .and_then(|m| m.gpu_type_id.as_deref())
            .unwrap_or("unknown")
    }
}
