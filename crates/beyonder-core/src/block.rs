use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

use crate::{AgentId, ProvenanceChain, SessionId, UnderlineStyle};

/// Unique, time-sortable block identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub String);

impl BlockId {
    pub fn new() -> Self {
        Self(Ulid::new().to_string())
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Block is the fundamental unit of content in Beyonder.
/// Replaces the traditional scroll buffer — every piece of content
/// (shell output, agent messages, approvals, diffs) is a Block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    pub parent_id: Option<BlockId>,
    pub agent_id: Option<AgentId>,
    pub session_id: SessionId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: BlockStatus,
    pub content: BlockContent,
    pub provenance: ProvenanceChain,
}

impl Block {
    pub fn new(
        kind: BlockKind,
        session_id: SessionId,
        content: BlockContent,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: BlockId::new(),
            kind,
            parent_id: None,
            agent_id: None,
            session_id,
            created_at: now,
            updated_at: now,
            status: BlockStatus::Pending,
            content,
            provenance: ProvenanceChain::default(),
        }
    }

    pub fn with_agent(mut self, agent_id: AgentId) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    pub fn with_parent(mut self, parent_id: BlockId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }
}

/// Categorizes what produced this block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Human,
    Agent,
    System,
    Tool,
    Approval,
}

/// Lifecycle status of a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// The structured content of a block.
/// This replaces unstructured text streams — every piece of content
/// has a typed representation that the terminal can render and reason about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockContent {
    /// Output from a shell command execution.
    ShellCommand {
        input: String,
        output: TerminalOutput,
        exit_code: Option<i32>,
        cwd: PathBuf,
        duration_ms: Option<u64>,
    },

    /// A message from an AI agent (ACP content blocks).
    AgentMessage {
        role: MessageRole,
        content_blocks: Vec<ContentBlock>,
    },

    /// An agent's tool call and its result.
    ToolCall {
        tool_name: String,
        tool_use_id: String,
        input: serde_json::Value,
        output: Option<String>,
        streaming_text: Option<String>,
        error: Option<String>,
        collapsed_default: bool,
    },

    /// A permission request that the human must approve or deny.
    ApprovalRequest {
        action: AgentAction,
        reasoning: Option<String>,
        granted: Option<bool>,
        granter: Option<ActorId>,
    },

    /// A file edit proposed by an agent.
    FileEdit {
        path: PathBuf,
        diff: UnifiedDiff,
        applied: bool,
    },

    /// An agent's structured plan.
    PlanNode {
        description: String,
        subtask_ids: Vec<BlockId>,
        progress: f32,
        is_complete: bool,
    },

    /// Plain text (e.g., system messages, banners).
    Text { text: String },
}

/// Parsed terminal output preserving ANSI color/style metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalOutput {
    /// Each cell is a character with optional styling.
    pub rows: Vec<TerminalRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRow {
    pub cells: Vec<TerminalCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalCell {
    #[serde(default, alias = "character", deserialize_with = "deser_grapheme_compat")]
    pub grapheme: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    #[serde(default, deserialize_with = "deser_underline_compat")]
    pub underline: UnderlineStyle,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Accepts either a single `char` (legacy format) or a `String` grapheme cluster.
fn deser_grapheme_compat<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Ch(char),
        Str(String),
    }
    match Either::deserialize(deserializer)? {
        Either::Ch(c) => Ok(c.to_string()),
        Either::Str(s) => Ok(s),
    }
}

/// Backward-compatible deserializer: accepts either the legacy `bool` form
/// (old DB rows where `underline` was a plain flag) or the new enum form.
fn deser_underline_compat<'de, D>(deserializer: D) -> Result<UnderlineStyle, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Bool(bool),
        Style(UnderlineStyle),
    }
    match Either::deserialize(deserializer)? {
        Either::Bool(true) => Ok(UnderlineStyle::Single),
        Either::Bool(false) => Ok(UnderlineStyle::None),
        Either::Style(s) => Ok(s),
    }
}

/// 24-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const GREEN: Self = Self { r: 80, g: 200, b: 120 };
    pub const RED: Self = Self { r: 220, g: 70, b: 70 };
    pub const BLUE: Self = Self { r: 80, g: 140, b: 220 };
    pub const YELLOW: Self = Self { r: 220, g: 200, b: 60 };
    pub const CYAN: Self = Self { r: 80, g: 200, b: 200 };
    pub const GRAY: Self = Self { r: 150, g: 150, b: 150 };

    pub fn to_wgpu(&self) -> [f32; 3] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]
    }
}

/// ACP content block types for agent messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Code { language: Option<String>, code: String },
    Thinking { thinking: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// An action an agent wants to take — used in ApprovalRequest blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentAction {
    FileWrite { path: PathBuf, content_preview: Option<String> },
    FileRead { path: PathBuf },
    FileDelete { path: PathBuf },
    ShellExecute { command: String },
    NetworkRequest { url: String, method: String },
    AgentSpawn { agent_name: String },
    ToolUse { tool_name: String },
}

/// A unified diff representing a file change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedDiff {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// An actor in the system — either a human user or an agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorId {
    Human,
    Agent { id: AgentId },
}
