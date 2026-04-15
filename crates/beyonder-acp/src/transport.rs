//! ACP transport layer — JSON-RPC framing over stdio.

use anyhow::Result;
use bytes::{Buf, BytesMut};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::messages::{JsonRpcRequest, JsonRpcResponse};

/// Sends JSON-RPC messages to an ACP agent over its stdin.
pub struct StdioSender {
    stdin: ChildStdin,
}

impl StdioSender {
    pub fn new(stdin: ChildStdin) -> Self {
        Self { stdin }
    }

    pub async fn send(&mut self, request: &JsonRpcRequest) -> Result<()> {
        let mut msg = serde_json::to_string(request)?;
        msg.push('\n');
        debug!(method = %request.method, "→ ACP request");
        self.stdin.write_all(msg.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

/// Reads JSON-RPC responses from an ACP agent's stdout.
/// Spawns a background task and sends parsed messages to a channel.
pub struct StdioReceiver {
    rx: mpsc::Receiver<Value>,
}

impl StdioReceiver {
    pub fn spawn(stdout: ChildStdout) -> Self {
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Value>(trimmed) {
                            Ok(val) => {
                                debug!("← ACP message");
                                if tx.send(val).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                error!(err = %e, raw = %trimmed, "Failed to parse ACP message");
                            }
                        }
                    }
                    Err(e) => {
                        error!(err = %e, "ACP stdout read error");
                        break;
                    }
                }
            }
        });
        Self { rx }
    }

    pub async fn recv(&mut self) -> Option<Value> {
        self.rx.recv().await
    }
}
