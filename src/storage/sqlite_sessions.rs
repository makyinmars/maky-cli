use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, anyhow};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    model::types::SessionMeta,
    storage::sessions::{SessionEvent, SessionRecord, SessionStore},
    util::ensure_parent_dir_exists,
};

const UNKNOWN_MODEL: &str = "unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteSessionStore {
    db_path: PathBuf,
}

impl Default for SqliteSessionStore {
    fn default() -> Self {
        Self::new(".maky/sessions.db")
    }
}

impl SqliteSessionStore {
    pub fn new<P>(db_path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            db_path: db_path.into(),
        }
    }

    fn open_connection(&self) -> anyhow::Result<Connection> {
        ensure_parent_dir_exists(&self.db_path)?;

        let connection = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open sqlite db at {}", self.db_path.display()))?;

        initialize_schema(&connection)?;
        Ok(connection)
    }
}

impl SessionStore for SqliteSessionStore {
    fn append_event(
        &self,
        session_id: &str,
        model: &str,
        event: &SessionEvent,
    ) -> anyhow::Result<()> {
        let mut connection = self.open_connection()?;
        let tx = connection
            .transaction()
            .context("failed to begin sqlite transaction")?;

        let now = unix_timestamp_seconds_i64();
        let normalized_model = if model.trim().is_empty() {
            UNKNOWN_MODEL
        } else {
            model.trim()
        };
        tx.execute(
            "INSERT INTO sessions (session_id, created_at, updated_at, model)
             VALUES (?1, ?2, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE
                 SET updated_at = excluded.updated_at,
                     model = excluded.model",
            params![session_id, now, normalized_model],
        )
        .with_context(|| format!("failed to upsert session metadata for {session_id}"))?;

        let next_sequence: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1
                 FROM session_events
                 WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .with_context(|| format!("failed to get next event sequence for {session_id}"))?;

        let payload_json = serde_json::to_string(event)
            .with_context(|| format!("failed to serialize session event for {session_id}"))?;

        tx.execute(
            "INSERT INTO session_events (session_id, sequence, event_type, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session_id,
                next_sequence,
                session_event_type(event),
                payload_json,
                now
            ],
        )
        .with_context(|| format!("failed to insert session event for {session_id}"))?;

        tx.commit().context("failed to commit sqlite transaction")?;
        Ok(())
    }

    fn load_session(&self, session_id: &str) -> anyhow::Result<Option<SessionRecord>> {
        let connection = self.open_connection()?;

        let meta_row = connection
            .query_row(
                "SELECT session_id, created_at, updated_at, model
                 FROM sessions
                 WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .with_context(|| format!("failed to load session metadata for {session_id}"))?;

        let Some((meta_session_id, created_at, updated_at, model)) = meta_row else {
            return Ok(None);
        };
        let meta = SessionMeta {
            session_id: meta_session_id,
            created_at: signed_to_unsigned_timestamp(created_at)?,
            updated_at: signed_to_unsigned_timestamp(updated_at)?,
            model,
        };

        let mut statement = connection
            .prepare(
                "SELECT payload_json
                 FROM session_events
                 WHERE session_id = ?1
                 ORDER BY sequence ASC",
            )
            .with_context(|| format!("failed to prepare load query for {session_id}"))?;

        let mut rows = statement
            .query(params![session_id])
            .with_context(|| format!("failed to query session events for {session_id}"))?;

        let mut events = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate session events")? {
            let payload_json: String = row.get(0).context("failed to read event payload")?;
            let event = serde_json::from_str::<SessionEvent>(&payload_json).with_context(|| {
                format!("failed to deserialize event payload for session {session_id}")
            })?;
            events.push(event);
        }

        Ok(Some(SessionRecord { meta, events }))
    }
}

fn initialize_schema(connection: &Connection) -> anyhow::Result<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS sessions (
                 session_id TEXT PRIMARY KEY,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 model TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS session_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 sequence INTEGER NOT NULL,
                 event_type TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY(session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_session_events_session_sequence
                 ON session_events (session_id, sequence);
             CREATE INDEX IF NOT EXISTS idx_session_events_session_id
                 ON session_events (session_id);",
        )
        .context("failed to initialize sqlite schema")
}

fn unix_timestamp_seconds_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

fn signed_to_unsigned_timestamp(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp out of range: {value}"))
}

fn session_event_type(event: &SessionEvent) -> &'static str {
    match event {
        SessionEvent::Message(_) => "message",
        SessionEvent::Provider(_) => "provider",
        SessionEvent::ToolResult(_) => "tool_result",
        SessionEvent::Status(_) => "status",
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::model::types::{Message, MessageRole};

    use super::*;

    #[test]
    fn append_and_load_session_round_trip() {
        let dir = tempdir().expect("tempdir should be created");
        let db_path = dir.path().join("sessions.db");
        let store = SqliteSessionStore::new(&db_path);

        let session_id = "session-round-trip";
        let message_event = SessionEvent::Message(Message {
            id: "msg-1".to_string(),
            role: MessageRole::User,
            content: "hello sqlite".to_string(),
            timestamp: 1_735_000_000,
        });
        let status_event = SessionEvent::Status("done".to_string());

        store
            .append_event(session_id, "openai/gpt-5.3-codex", &message_event)
            .expect("message append should succeed");
        store
            .append_event(session_id, "openai/gpt-5.3-codex", &status_event)
            .expect("status append should succeed");

        let record = store
            .load_session(session_id)
            .expect("load should succeed")
            .expect("record should exist");

        assert_eq!(record.meta.session_id, session_id);
        assert_eq!(record.meta.model, "openai/gpt-5.3-codex");
        assert_eq!(record.events, vec![message_event, status_event]);
        assert!(record.meta.updated_at >= record.meta.created_at);
    }

    #[test]
    fn load_missing_session_returns_none() {
        let dir = tempdir().expect("tempdir should be created");
        let db_path = dir.path().join("sessions.db");
        let store = SqliteSessionStore::new(&db_path);

        let result = store
            .load_session("missing-session")
            .expect("load should succeed");

        assert!(result.is_none());
    }
}
