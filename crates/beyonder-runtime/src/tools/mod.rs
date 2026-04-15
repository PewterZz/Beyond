use async_trait::async_trait;
use beyonder_core::{AgentAction, AgentId, SessionId};
use tokio_util::sync::CancellationToken;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: false }
    }
    pub fn error(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: true }
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": if self.is_error { "error" } else { "text" },
            "text": self.content,
        })
    }
}

/// Context available to tool implementations during execution.
#[derive(Clone)]
pub struct ToolContext {
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub cwd: std::path::PathBuf,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
    fn required_action(&self, input: &serde_json::Value) -> AgentAction;
    fn collapsed_default(&self) -> bool { false }
    async fn execute(&self, input: serde_json::Value, ctx: ToolContext, cancel: CancellationToken) -> Result<ToolOutput>;
}

/// A pending tool execution request sent from the supervisor to the app layer.
pub struct ToolExecRequest {
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub tool_use_id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub result_tx: tokio::sync::oneshot::Sender<ToolOutput>,
}

pub mod registry;
pub mod executor;
pub mod shell;
