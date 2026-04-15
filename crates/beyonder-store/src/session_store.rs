use beyonder_core::{Session, SessionId};
use rusqlite::params;

use crate::{Store, StoreError, StoreResult};

pub struct SessionStore<'a> {
    store: &'a Store,
}

impl<'a> SessionStore<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub fn insert(&self, session: &Session) -> StoreResult<()> {
        let data = serde_json::to_string(session)?;
        self.store.conn.execute(
            r#"INSERT INTO sessions (id, name, created_at, last_active, working_dir, status, data)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                session.id.0,
                session.name.as_deref(),
                session.created_at.to_rfc3339(),
                session.last_active.to_rfc3339(),
                session.working_directory.to_string_lossy().as_ref(),
                format!("{:?}", session.status),
                data,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &SessionId) -> StoreResult<Session> {
        let data: String = self
            .store
            .conn
            .query_row(
                "SELECT data FROM sessions WHERE id = ?1",
                params![id.0],
                |row| row.get(0),
            )
            .map_err(|_| StoreError::NotFound(id.0.clone()))?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn list_active(&self) -> StoreResult<Vec<Session>> {
        let mut stmt = self.store.conn.prepare(
            "SELECT data FROM sessions WHERE status = 'Active' ORDER BY last_active DESC",
        )?;
        let sessions = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect();
        Ok(sessions)
    }

    pub fn update(&self, session: &Session) -> StoreResult<()> {
        let data = serde_json::to_string(session)?;
        self.store.conn.execute(
            "UPDATE sessions SET last_active = ?1, status = ?2, data = ?3 WHERE id = ?4",
            params![
                session.last_active.to_rfc3339(),
                format!("{:?}", session.status),
                data,
                session.id.0,
            ],
        )?;
        Ok(())
    }
}
