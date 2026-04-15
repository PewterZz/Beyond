//! LLM provider backends. The `AgentBackend` trait unifies ACP subprocesses
//! and direct Ollama calls so the supervisor's drive_turn loop stays
//! backend-agnostic.

use anyhow::Result;
use async_trait::async_trait;
use beyonder_acp::client::StreamPause;

pub mod ollama;
pub use ollama::{OllamaBackend, OllamaConfig, ToolDescriptor};

/// Uniform turn-driving interface.
#[async_trait]
pub trait AgentBackend: Send {
    /// Begin a new user turn (append user message / send initial prompt).
    async fn start_turn(&mut self, user_text: &str) -> Result<()>;

    /// Drive the stream until the next pause point. Text deltas are emitted
    /// via the backend's event channel during this call.
    async fn stream_until_pause(&mut self) -> Result<StreamPause>;

    /// Submit tool results back to the agent; caller will then call
    /// `stream_until_pause` again to continue the turn.
    async fn submit_tool_results(&mut self, results: &[(String, serde_json::Value)]) -> Result<()>;

    /// Reset the conversation history back to the initial state (system prompt only).
    /// Called when the user clears the terminal so the agent starts fresh.
    async fn reset_conversation(&mut self) {}
}

/// Implement AgentBackend for AcpClient. The trait lives in this crate,
/// so the orphan rule is satisfied.
#[async_trait]
impl AgentBackend for beyonder_acp::AcpClient {
    async fn start_turn(&mut self, user_text: &str) -> Result<()> {
        self.start_prompt(user_text).await
    }
    async fn stream_until_pause(&mut self) -> Result<StreamPause> {
        self.stream_until_pause().await
    }
    async fn submit_tool_results(&mut self, results: &[(String, serde_json::Value)]) -> Result<()> {
        self.submit_tool_results(results).await
    }
}
