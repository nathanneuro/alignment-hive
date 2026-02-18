use reqwest::Client;

use super::types::{Pod, PodCreateInput};

const BASE_URL: &str = "https://rest.runpod.io/v1";

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

    pub async fn create_pod(&self, input: &PodCreateInput) -> anyhow::Result<Pod> {
        tracing::debug!(request = %serde_json::to_string_pretty(input)?, "Creating pod");

        let resp = self
            .client
            .post(format!("{BASE_URL}/pods"))
            .bearer_auth(&self.api_key)
            .json(input)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("RunPod API error ({status}): {body}");
        }

        tracing::debug!(%body, "Create pod response");
        Ok(serde_json::from_str(&body)?)
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
