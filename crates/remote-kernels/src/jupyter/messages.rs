use serde::{Deserialize, Serialize};

/// Jupyter WebSocket message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupyterMessage {
    pub channel: String,
    pub header: Header,
    #[serde(default)]
    pub parent_header: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub content: serde_json::Value,
    #[serde(default)]
    pub buffers: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub msg_id: String,
    pub msg_type: String,
    #[serde(default = "default_username")]
    pub username: String,
    pub session: String,
    #[serde(default)]
    pub date: String,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_username() -> String {
    "remote-kernels".to_string()
}

fn default_version() -> String {
    "5.3".to_string()
}

impl JupyterMessage {
    /// Create an `execute_request` message.
    pub fn execute_request(session_id: &str, code: &str) -> Self {
        Self {
            channel: "shell".to_string(),
            header: Header {
                msg_id: uuid::Uuid::new_v4().to_string(),
                msg_type: "execute_request".to_string(),
                username: "remote-kernels".to_string(),
                session: session_id.to_string(),
                date: String::new(),
                version: "5.3".to_string(),
            },
            parent_header: serde_json::Value::Object(serde_json::Map::new()),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            content: serde_json::json!({
                "code": code,
                "silent": false,
                "store_history": true,
                "user_expressions": {},
                "allow_stdin": false,
                "stop_on_error": true
            }),
            buffers: vec![],
        }
    }
}

/// Parsed output from a kernel execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionOutput {
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub error: Option<ErrorInfo>,
    pub display_data: Vec<String>,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ExecutionStatus {
    #[default]
    Running,
    Complete,
    Errored,
}

#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub ename: String,
    pub evalue: String,
    pub traceback: Vec<String>,
}

impl ExecutionOutput {
    /// Create an error output with a message.
    pub fn error(msg: String) -> Self {
        Self {
            stderr: msg,
            status: ExecutionStatus::Errored,
            ..Default::default()
        }
    }

    /// Format the output for display to the user.
    pub fn format(&self) -> String {
        let mut parts = Vec::new();

        if !self.stdout.is_empty() {
            parts.push(self.stdout.clone());
        }

        if !self.stderr.is_empty() {
            parts.push(format!("[stderr]\n{}", self.stderr));
        }

        if let Some(ref result) = self.result {
            parts.push(result.clone());
        }

        for data in &self.display_data {
            parts.push(data.clone());
        }

        if let Some(ref err) = self.error {
            let tb = err.traceback.join("\n");
            parts.push(format!("{}: {}\n{tb}", err.ename, err.evalue));
        }

        if parts.is_empty() {
            "(no output)".to_string()
        } else {
            parts.join("\n")
        }
    }

    /// Process an incoming iopub message, updating the output state.
    pub fn process_iopub(&mut self, msg: &JupyterMessage) {
        let msg_type = msg.header.msg_type.as_str();
        match msg_type {
            "stream" => {
                let name = msg.content["name"].as_str().unwrap_or("stdout");
                let text = msg.content["text"].as_str().unwrap_or("");
                if name == "stderr" {
                    self.stderr.push_str(text);
                } else {
                    self.stdout.push_str(text);
                }
            }
            "execute_result" => {
                if let Some(text) = msg.content["data"]["text/plain"].as_str() {
                    self.result = Some(text.to_string());
                }
            }
            "display_data" => {
                // Prefer text/plain for now; images would need base64 handling.
                if let Some(text) = msg.content["data"]["text/plain"].as_str() {
                    self.display_data.push(text.to_string());
                }
            }
            "error" => {
                let ename = msg.content["ename"].as_str().unwrap_or("Error").to_string();
                let evalue = msg.content["evalue"].as_str().unwrap_or("").to_string();
                let traceback = msg.content["traceback"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                self.error = Some(ErrorInfo {
                    ename,
                    evalue,
                    traceback,
                });
                self.status = ExecutionStatus::Errored;
            }
            _ => {}
        }
    }
}
