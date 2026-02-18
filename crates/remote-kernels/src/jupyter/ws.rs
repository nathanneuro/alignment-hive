use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use super::messages::{ExecutionOutput, ExecutionStatus, JupyterMessage};

/// A handle to a WebSocket connection to a single kernel.
pub struct KernelConnection {
    /// Send execute requests.
    request_tx: mpsc::Sender<ExecuteCommand>,
    /// Background reader task handle.
    _reader_handle: tokio::task::JoinHandle<()>,
}

struct ExecuteCommand {
    msg: JupyterMessage,
    result_tx: oneshot::Sender<ExecutionOutput>,
}

impl KernelConnection {
    /// Connect to a kernel's WebSocket channels endpoint.
    pub async fn connect(pod_id: &str, kernel_id: &str, token: &str) -> anyhow::Result<Self> {
        let url = format!(
            "wss://{pod_id}-8888.proxy.runpod.net/api/kernels/{kernel_id}/channels?token={token}"
        );

        tracing::debug!(%kernel_id, "Connecting to kernel WebSocket");

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;
        let (ws_sink, ws_stream_rx) = ws_stream.split();

        let (request_tx, request_rx) = mpsc::channel::<ExecuteCommand>(16);

        let reader_handle = tokio::spawn(Self::run_ws_loop(ws_sink, ws_stream_rx, request_rx));

        tracing::info!(%kernel_id, "Connected to kernel WebSocket");

        Ok(Self {
            request_tx,
            _reader_handle: reader_handle,
        })
    }

    /// Background task: handles sending requests and reading responses.
    #[allow(clippy::too_many_lines)]
    async fn run_ws_loop(
        mut ws_sink: futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        mut ws_stream_rx: futures::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        mut request_rx: mpsc::Receiver<ExecuteCommand>,
    ) {
        let mut pending: Option<(String, ExecutionOutput, oneshot::Sender<ExecutionOutput>)> = None;

        loop {
            tokio::select! {
                Some(cmd) = request_rx.recv() => {
                    let msg_id = cmd.msg.header.msg_id.clone();
                    let json = match serde_json::to_string(&cmd.msg) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!("Failed to serialize execute_request: {e}");
                            let _ = cmd.result_tx.send(ExecutionOutput::error(format!("Internal error: {e}")));
                            continue;
                        }
                    };

                    if let Err(e) = ws_sink.send(Message::Text(json.into())).await {
                        tracing::error!("Failed to send WebSocket message: {e}");
                        let _ = cmd.result_tx.send(ExecutionOutput::error(format!("WebSocket send error: {e}")));
                        continue;
                    }

                    pending = Some((msg_id, ExecutionOutput::default(), cmd.result_tx));
                }

                Some(msg_result) = ws_stream_rx.next() => {
                    let msg = match msg_result {
                        Ok(Message::Text(text)) => {
                            match serde_json::from_str::<JupyterMessage>(&text) {
                                Ok(m) => m,
                                Err(e) => {
                                    tracing::debug!("Ignoring unparseable WS message: {e}");
                                    continue;
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            tracing::info!("WebSocket closed");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {e}");
                            break;
                        }
                        _ => continue,
                    };

                    let parent_msg_id = msg.parent_header
                        .get("msg_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if let Some((ref expected_id, ref mut output, _)) = pending {
                        if parent_msg_id != expected_id {
                            continue;
                        }
                        match msg.channel.as_str() {
                            "iopub" => {
                                output.process_iopub(&msg);
                            }
                            "shell" if msg.header.msg_type == "execute_reply" => {
                                let status = msg.content["status"].as_str().unwrap_or("ok");
                                if status == "error" && output.status != ExecutionStatus::Errored {
                                    output.status = ExecutionStatus::Errored;
                                } else if output.status == ExecutionStatus::Running {
                                    output.status = ExecutionStatus::Complete;
                                }

                                if let Some((_, output, tx)) = pending.take() {
                                    let _ = tx.send(output);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                else => break,
            }
        }

        // If we exit with a pending execution, complete it with what we have.
        if let Some((_, mut output, tx)) = pending.take() {
            if output.status == ExecutionStatus::Running {
                output.status = ExecutionStatus::Errored;
                output
                    .stderr
                    .push_str("\nWebSocket connection closed unexpectedly.");
            }
            let _ = tx.send(output);
        }
    }

    /// Execute code and wait for the result.
    pub async fn execute(
        &self,
        session_id: &str,
        code: &str,
        timeout: std::time::Duration,
    ) -> anyhow::Result<ExecutionOutput> {
        let msg = JupyterMessage::execute_request(session_id, code);
        let (result_tx, result_rx) = oneshot::channel();

        self.request_tx
            .send(ExecuteCommand { msg, result_tx })
            .await
            .map_err(|_| anyhow::anyhow!("Kernel connection closed"))?;

        match tokio::time::timeout(timeout, result_rx).await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(_)) => anyhow::bail!("Kernel connection dropped before execution completed"),
            Err(_) => Ok(ExecutionOutput {
                stderr: "Execution timed out. The code may still be running on the kernel."
                    .to_string(),
                ..Default::default()
            }),
        }
    }
}
