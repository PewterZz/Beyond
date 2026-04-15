use rusqlite::Connection;

use crate::StoreResult;

pub fn run(conn: &Connection) -> StoreResult<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    name        TEXT,
    created_at  TEXT NOT NULL,
    last_active TEXT NOT NULL,
    working_dir TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',
    data        TEXT NOT NULL  -- JSON-serialized Session
);

CREATE TABLE IF NOT EXISTS blocks (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id),
    kind        TEXT NOT NULL,
    status      TEXT NOT NULL,
    agent_id    TEXT,
    parent_id   TEXT REFERENCES blocks(id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    data        TEXT NOT NULL  -- JSON-serialized Block
);

CREATE INDEX IF NOT EXISTS idx_blocks_session ON blocks(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_blocks_agent   ON blocks(agent_id);
CREATE INDEX IF NOT EXISTS idx_blocks_parent  ON blocks(parent_id);

CREATE TABLE IF NOT EXISTS agents (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    name       TEXT NOT NULL,
    state      TEXT NOT NULL,
    data       TEXT NOT NULL  -- JSON-serialized AgentInfo
);

CREATE TABLE IF NOT EXISTS capability_grants (
    id         TEXT PRIMARY KEY,
    agent_id   TEXT NOT NULL REFERENCES agents(id),
    block_id   TEXT REFERENCES blocks(id),  -- approval block that granted this
    granted_at TEXT NOT NULL,
    data       TEXT NOT NULL  -- JSON-serialized Capability
);
"#;
