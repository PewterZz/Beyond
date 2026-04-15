//! Beyonder-specific ACP extensions (beyonder/* namespace).

use serde::{Deserialize, Serialize};

/// Agent status query request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusParams {
    pub agent_id: String,
}

/// Agent status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusResult {
    pub agent_id: String,
    pub state: String,
    pub tokens_used: u64,
    pub actions_taken: u32,
}

/// Capability request from agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequestParams {
    pub capability_kind: String,
    pub scope: Option<String>,
    pub reasoning: Option<String>,
}

/// Block query request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockQueryParams {
    pub session_id: String,
    pub limit: Option<u32>,
    pub kind_filter: Option<Vec<String>>,
}
