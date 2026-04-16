//! Wire protocol for the /phone bridge.
//!
//! Frames are length-prefixed CBOR blobs carried inside binary WebSocket
//! messages. CBOR was picked over JSON because the iPhone client decodes it
//! ~3× faster and it preserves the exact byte-level representation of
//! `BlockContent` variants without the round-trip cost.

use beyonder_core::{Block, BlockId, BlockStatus};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub version: u16,
    pub server_name: String,
    pub session_id: String,
    pub active_model: String,
    pub active_provider: String,
}

/// Server → phone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum ServerMsg {
    Hello(Hello),
    /// Full block — sent when a new block appears.
    BlockAppended(Block),
    /// Status / content update on an existing block (used for streaming agent text).
    BlockUpdated {
        id: BlockId,
        status: BlockStatus,
        content_patch: Option<ContentPatch>,
    },
    /// Incremental append to a streaming agent text block.
    AgentTextDelta {
        block_id: BlockId,
        delta: String,
    },
    /// Agent completed its turn — phone can stop showing the spinner.
    AgentTurnComplete {
        agent_id: String,
        stop_reason: String,
    },
    Pong {
        nonce: u64,
    },
    Error {
        message: String,
    },
    /// Live PTY frame mirror — full-screen snapshot of the Mac terminal grid.
    /// Phone renders this directly so TUIs (vim, claude, htop, etc.) work.
    PtyFrame(PtyFrame),
    /// Full list of tabs — sent on connect and whenever tabs change.
    TabList(TabList),
    /// Active tab changed (index into the tab list).
    TabSwitched { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabList {
    pub tabs: Vec<TabInfo>,
    pub active: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub index: usize,
    pub title: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyFrame {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cells: Vec<Vec<PtyCell>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyCell {
    pub g: String,
    pub fg: [u8; 3],
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
}

/// Phone → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum ClientMsg {
    /// Auth: bearer token (pre-shared via pairing QR).
    Auth { token: String },
    /// Free-form prompt — goes through the normal agent path.
    Prompt { text: String },
    /// Direct shell command — bypasses agent.
    RunCommand { cmd: String },
    /// Pre-parsed tool-call hint from the on-device model.
    /// Server may use this to short-circuit planning if safe.
    ToolHint { name: String, args_json: String },
    /// Interrupt the current in-flight agent turn.
    Interrupt,
    Ping { nonce: u64 },
    /// Raw bytes to write to the Mac PTY stdin (individual keystrokes,
    /// including ANSI escape sequences for arrows / Ctrl-keys).
    PtyInput { bytes: Vec<u8> },
    /// Phone tells the Mac to resize the PTY + TermGrid to dims that fit
    /// the phone screen. TUIs re-render to the new size.
    PtyResize { cols: u16, rows: u16 },
    /// Phone requests switching to a different tab.
    SwitchTab { index: usize },
    /// Phone requests opening a new tab.
    NewTab,
    /// Phone requests closing the active tab.
    CloseTab,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPatch {
    pub text_append: Option<String>,
}
