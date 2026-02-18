use reqwest::Client;
use serde::Deserialize;

/// Client for the Jupyter Server REST API.
pub struct JupyterClient {
    client: Client,
    base_url: String,
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KernelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub execution_state: Option<String>,
    #[serde(default)]
    pub last_activity: Option<String>,
}

impl JupyterClient {
    pub fn new(pod_id: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("https://{pod_id}-8888.proxy.runpod.net"),
            token: token.to_string(),
        }
    }

    /// Poll until Jupyter server is reachable.
    pub async fn wait_until_ready(&self) -> anyhow::Result<()> {
        let url = format!("{}/api", self.base_url);
        let mut attempts = 0;

        loop {
            attempts += 1;
            match self
                .client
                .get(&url)
                .header("Authorization", format!("token {}", self.token))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(attempts, "Jupyter server is ready");
                    return Ok(());
                }
                Ok(resp) => {
                    tracing::debug!(
                        attempts,
                        status = %resp.status(),
                        "Jupyter not ready yet"
                    );
                }
                Err(e) => {
                    tracing::debug!(attempts, error = %e, "Jupyter not reachable yet");
                }
            }

            if attempts > 60 {
                anyhow::bail!("Jupyter server did not become ready after 3 minutes");
            }

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }

    /// Create a new Python kernel, returns its ID.
    pub async fn create_kernel(&self) -> anyhow::Result<KernelInfo> {
        let url = format!("{}/api/kernels", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.token))
            .json(&serde_json::json!({"name": "python3"}))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create kernel ({status}): {body}");
        }

        let kernel: KernelInfo = resp.json().await?;
        tracing::info!(kernel_id = %kernel.id, "Created kernel");
        Ok(kernel)
    }

    /// List all running kernels.
    pub async fn list_kernels(&self) -> anyhow::Result<Vec<KernelInfo>> {
        let url = format!("{}/api/kernels", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list kernels ({status}): {body}");
        }

        Ok(resp.json().await?)
    }

    /// Delete (shut down) a kernel.
    pub async fn delete_kernel(&self, kernel_id: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/kernels/{kernel_id}", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to delete kernel ({status}): {body}");
        }

        tracing::info!(kernel_id, "Deleted kernel");
        Ok(())
    }

    /// Restart a kernel (preserves kernel ID, clears state).
    pub async fn restart_kernel(&self, kernel_id: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/kernels/{kernel_id}/restart", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to restart kernel ({status}): {body}");
        }

        tracing::info!(kernel_id, "Restarted kernel");
        Ok(())
    }

    /// Interrupt a running execution in a kernel.
    pub async fn interrupt_kernel(&self, kernel_id: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/kernels/{kernel_id}/interrupt", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to interrupt kernel ({status}): {body}");
        }

        tracing::info!(kernel_id, "Interrupted kernel");
        Ok(())
    }
}
