// RunPod API types.
//
// REST API (rest.runpod.io/v1): used for pod CRUD. OpenAPI spec at /v1/openapi.json.
// GraphQL API (api.runpod.io/graphql): used for runtime info (ports, GPU stats).
//   Schema browser: https://graphql-spec.runpod.io/
//
// The REST API does NOT return runtime networking info (port mappings, public IP).
// Use the GraphQL API for SSH connection details.

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
    /// Extra fields passed through from [runpod] config section.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
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
    pub ports: Option<Vec<String>>,
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

// --- GraphQL response types (api.runpod.io/graphql) ---

/// Wrapper for GraphQL `{ data: { pod: ... } }` responses.
#[derive(Debug, Deserialize)]
pub struct GraphQlResponse<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlError {
    pub message: String,
}

/// The `pod` field from a GraphQL query.
#[derive(Debug, Deserialize)]
pub struct GraphQlPodData {
    pub pod: Option<GraphQlPod>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlPod {
    pub runtime: Option<PodRuntime>,
}

/// Runtime info from the GraphQL API (only populated while pod is running).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodRuntime {
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub ports: Vec<PodRuntimePort>,
}

/// Deserialize `null` as the default value for a type.
/// `#[serde(default)]` only handles missing fields, not explicit `null` values.
fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// A single port mapping from the GraphQL runtime.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodRuntimePort {
    pub ip: String,
    pub is_ip_public: bool,
    pub private_port: u16,
    pub public_port: u16,
    #[serde(rename = "type")]
    pub port_type: String,
}
