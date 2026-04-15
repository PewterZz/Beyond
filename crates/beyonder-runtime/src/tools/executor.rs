use super::{registry::ToolRegistry, ToolContext, ToolExecRequest, ToolOutput};
use crate::capability_broker::{ActionDecision, ApprovalDecision, CapabilityBroker};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct ToolExecutor {
    pub registry: ToolRegistry,
}

impl ToolExecutor {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }

    /// Execute a single tool call, checking capabilities and running the tool.
    pub async fn run(
        &self,
        request: ToolExecRequest,
        broker: &mut CapabilityBroker,
        cwd: std::path::PathBuf,
    ) -> ToolOutput {
        let tool = match self.registry.get(&request.tool_name) {
            Some(t) => Arc::clone(t),
            None => return ToolOutput::error(format!("Unknown tool: {}", request.tool_name)),
        };

        let ctx = ToolContext {
            agent_id: request.agent_id.clone(),
            session_id: request.session_id.clone(),
            cwd,
        };

        let action = tool.required_action(&request.input);

        let decision = broker
            .check_action(&request.agent_id, &action, &request.session_id)
            .await;

        match decision {
            ActionDecision::Denied(reason) => {
                ToolOutput::error(format!("Permission denied: {reason}"))
            }
            ActionDecision::Approved => {
                let cancel = CancellationToken::new();
                match tool.execute(request.input, ctx, cancel).await {
                    Ok(output) => output,
                    Err(e) => ToolOutput::error(format!("Tool execution error: {e}")),
                }
            }
            ActionDecision::NeedsApproval { approval_rx } => match approval_rx.await {
                Ok(ApprovalDecision::Granted) | Ok(ApprovalDecision::GrantedAlways) => {
                    let cancel = CancellationToken::new();
                    match tool.execute(request.input, ctx, cancel).await {
                        Ok(output) => output,
                        Err(e) => ToolOutput::error(format!("Tool execution error: {e}")),
                    }
                }
                Ok(ApprovalDecision::Denied) => {
                    ToolOutput::error("Permission denied by user".to_string())
                }
                Err(_) => ToolOutput::error("Approval channel closed".to_string()),
            },
        }
    }
}
