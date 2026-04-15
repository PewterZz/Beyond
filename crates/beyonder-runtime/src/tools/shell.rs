use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use beyonder_core::AgentAction;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub struct ShellExec;

#[async_trait]
impl Tool for ShellExec {
    fn name(&self) -> &'static str {
        "shell.exec"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command and return its output."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string", "description": "The shell command to execute" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds (default 30000)" }
            },
            "required": ["cmd"]
        })
    }

    fn required_action(&self, input: &Value) -> AgentAction {
        let cmd = input
            .get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        AgentAction::ShellExecute { command: cmd }
    }

    fn collapsed_default(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        input: Value,
        ctx: ToolContext,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let cmd = match input.get("cmd").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => return Ok(ToolOutput::error("Missing required field: cmd")),
        };
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .unwrap_or(ctx.cwd.clone());
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);

        let mut child = match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .current_dir(&cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return Ok(ToolOutput::error(format!("Failed to spawn: {e}"))),
        };

        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        // Take stdout/stderr handles before consuming child in select!.
        use tokio::io::AsyncReadExt;
        let mut stdout_handle = match child.stdout.take() {
            Some(h) => h,
            None => return Ok(ToolOutput::error("Failed to capture stdout")),
        };
        let mut stderr_handle = match child.stderr.take() {
            Some(h) => h,
            None => return Ok(ToolOutput::error("Failed to capture stderr")),
        };

        let result = tokio::select! {
            status = child.wait() => {
                let mut stdout_bytes = Vec::new();
                let mut stderr_bytes = Vec::new();
                // Drain any remaining bytes (best effort — may already be done).
                let _ = stdout_handle.read_to_end(&mut stdout_bytes).await;
                let _ = stderr_handle.read_to_end(&mut stderr_bytes).await;
                match status {
                    Ok(status) => {
                        let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
                        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
                        let combined = if stderr.is_empty() {
                            stdout
                        } else if stdout.is_empty() {
                            stderr
                        } else {
                            format!("{stdout}\n{stderr}")
                        };
                        let exit_code = status.code().unwrap_or(-1);
                        if status.success() {
                            ToolOutput::ok(combined)
                        } else {
                            ToolOutput::error(format!("exit code {exit_code}\n{combined}"))
                        }
                    }
                    Err(e) => ToolOutput::error(format!("Process error: {e}")),
                }
            }
            _ = tokio::time::sleep(timeout) => {
                child.kill().await.ok();
                ToolOutput::error(format!("Command timed out after {timeout_ms}ms"))
            }
            _ = cancel.cancelled() => {
                child.kill().await.ok();
                ToolOutput::error("Cancelled".to_string())
            }
        };

        Ok(result)
    }
}
