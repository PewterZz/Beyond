use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ActorId, BlockId};

/// A causal chain tracking how a block came to exist.
/// Answers: "Agent X produced this because of Y, authorized by Z."
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceChain {
    pub nodes: Vec<ProvenanceNode>,
}

impl ProvenanceChain {
    pub fn with_cause(mut self, cause: CauseKind, actor: ActorId) -> Self {
        self.nodes.push(ProvenanceNode {
            cause,
            actor,
            timestamp: Utc::now(),
        });
        self
    }

    /// The immediate cause — the last node in the chain.
    pub fn immediate_cause(&self) -> Option<&ProvenanceNode> {
        self.nodes.last()
    }

    /// The root cause — the first node (usually a human prompt).
    pub fn root_cause(&self) -> Option<&ProvenanceNode> {
        self.nodes.first()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceNode {
    pub cause: CauseKind,
    pub actor: ActorId,
    pub timestamp: DateTime<Utc>,
}

/// The kind of causal link in the provenance chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CauseKind {
    /// Triggered by a human user prompt.
    HumanPrompt { prompt_summary: String },
    /// Triggered by another block's content.
    BlockOutput { block_id: BlockId },
    /// Triggered by a tool call result.
    ToolResult { tool_name: String },
    /// Authorized by a human approval action.
    ApprovalGranted { approval_block_id: BlockId },
    /// Triggered by agent's own planning.
    AgentPlan { plan_description: String },
    /// System-initiated (e.g., startup, auto-resume).
    System { reason: String },
}
