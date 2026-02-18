use reqwest::Client;

use super::types::{Pod, PodCreateInput};

const BASE_URL: &str = "https://rest.runpod.io/v1";

#[derive(Debug, thiserror::Error)]
pub enum RunPodError {
    #[error("RunPod API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl RunPodError {
    pub fn is_server_error(&self) -> bool {
        matches!(self, Self::Api { status, .. } if *status >= 500)
    }
}

pub struct RunPodClient {
    client: Client,
    api_key: String,
}

impl RunPodClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    pub async fn create_pod(&self, input: &PodCreateInput) -> Result<Pod, RunPodError> {
        tracing::debug!(request = %serde_json::to_string_pretty(input).unwrap_or_default(), "Creating pod");

        let resp = self
            .client
            .post(format!("{BASE_URL}/pods"))
            .bearer_auth(&self.api_key)
            .json(input)
            .send()
            .await
            .map_err(|e| RunPodError::Other(e.into()))?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(RunPodError::Api {
                status: status.as_u16(),
                body,
            });
        }

        tracing::debug!(%body, "Create pod response");
        serde_json::from_str(&body).map_err(|e| RunPodError::Other(e.into()))
    }

    pub async fn get_pod(&self, pod_id: &str) -> anyhow::Result<Pod> {
        let resp = self
            .client
            .get(format!("{BASE_URL}/pods/{pod_id}"))
            .bearer_auth(&self.api_key)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("RunPod API error ({status}): {body}");
        }

        tracing::debug!(%body, "Get pod response");
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn stop_pod(&self, pod_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .post(format!("{BASE_URL}/pods/{pod_id}/stop"))
            .bearer_auth(&self.api_key)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("RunPod API error ({status}): {body}");
        }

        Ok(())
    }

    pub async fn terminate_pod(&self, pod_id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .delete(format!("{BASE_URL}/pods/{pod_id}"))
            .bearer_auth(&self.api_key)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("RunPod API error ({status}): {body}");
        }

        Ok(())
    }
}
