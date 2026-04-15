use beyonder_core::{Block, BlockId, BlockStatus, SessionId};
use rusqlite::params;

use crate::{Store, StoreError, StoreResult};

pub struct BlockStore<'a> {
    store: &'a Store,
}

impl<'a> BlockStore<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn insert(&self, block: &Block) -> StoreResult<()> {
        let data = serde_json::to_string(block)?;
        self.store.conn.execute(
            r#"INSERT INTO blocks (id, session_id, kind, status, agent_id, parent_id, created_at, updated_at, data)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                block.id.0,
                block.session_id.0,
                format!("{:?}", block.kind),
                format!("{:?}", block.status),
                block.agent_id.as_ref().map(|a| &a.0),
                block.parent_id.as_ref().map(|p| &p.0),
                block.created_at.to_rfc3339(),
                block.updated_at.to_rfc3339(),
                data,
            ],
        )?;
        Ok(())
    }

    pub fn update(&self, block: &Block) -> StoreResult<()> {
        let data = serde_json::to_string(block)?;
        self.store.conn.execute(
            "UPDATE blocks SET status = ?1, updated_at = ?2, data = ?3 WHERE id = ?4",
            params![
                format!("{:?}", block.status),
                block.updated_at.to_rfc3339(),
                data,
                block.id.0,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &BlockId) -> StoreResult<Block> {
        let data: String = self
            .store
            .conn
            .query_row(
                "SELECT data FROM blocks WHERE id = ?1",
                params![id.0],
                |row| row.get(0),
            )
            .map_err(|_| StoreError::NotFound(id.0.clone()))?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn list_for_session(&self, session_id: &SessionId) -> StoreResult<Vec<Block>> {
        let mut stmt = self
            .store
            .conn
            .prepare("SELECT data FROM blocks WHERE session_id = ?1 ORDER BY created_at ASC")?;
        let blocks = stmt
            .query_map(params![session_id.0], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect();
        Ok(blocks)
    }

    pub fn update_status(&self, id: &BlockId, status: &BlockStatus) -> StoreResult<()> {
        self.store.conn.execute(
            "UPDATE blocks SET status = ?1 WHERE id = ?2",
            params![format!("{:?}", status), id.0],
        )?;
        Ok(())
    }
}
