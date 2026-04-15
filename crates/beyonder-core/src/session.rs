use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

/// Unique session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Ulid::new().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Beyonder session — contains a shell, zero or more agents, and a block stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub working_directory: PathBuf,
    pub shell: ShellConfig,
    pub status: SessionStatus,
    /// If this session was forked from another, records the parent.
    pub forked_from: Option<(SessionId, String)>,
}

impl Session {
    pub fn new(working_directory: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            name: None,
            created_at: now,
            last_active: now,
            working_directory,
            shell: ShellConfig::default(),
            status: SessionStatus::Active,
            forked_from: None,
        }
    }
}

/// Shell configuration for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        Self {
            program: shell,
            args: vec![],
            env: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Suspended,
    Closed,
}
