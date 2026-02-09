/// SQLite-backed persistence for chat sessions and messages.
///
/// Each `Session` groups a sequence of `ChatMessage`s tied to a particular
/// model. The store exposes a simple CRUD API that is safe to call from async
/// Tokio code — all SQLite operations are wrapped in `spawn_blocking` since
/// `rusqlite::Connection` is not `Send` across await points.
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::types::{ChatMessage, MessageContent, MessageRole, Session};

// ---------------------------------------------------------------------------
// SessionStore
// ---------------------------------------------------------------------------

pub struct SessionStore {
    conn: Mutex<Connection>,
}

impl SessionStore {
    // -- Construction & migrations ------------------------------------------

    /// Open (or create) the SQLite database at `db_path` and run migrations.
    ///
    /// Parent directories are created automatically if they do not exist.
    pub fn new(db_path: &str) -> Result<Self, String> {
        // Ensure the parent directory exists.
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create db directory: {e}"))?;
        }

        let conn =
            Connection::open(db_path).map_err(|e| format!("cannot open database: {e}"))?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("pragma error: {e}"))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create the tables and indices if they do not already exist.
    fn run_migrations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                model_id    TEXT    NOT NULL,
                model_name  TEXT    NOT NULL,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id            TEXT PRIMARY KEY,
                session_id    TEXT    NOT NULL,
                role          TEXT    NOT NULL,
                content_json  TEXT    NOT NULL,
                timestamp     INTEGER NOT NULL,
                inference_ms  INTEGER,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id, timestamp);
            ",
        )
        .map_err(|e| format!("migration error: {e}"))?;

        // Add display_name column for user-defined session names (idempotent).
        let has_display_name: bool = conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'display_name'")
            .and_then(|mut s| s.query_row([], |r| r.get::<_, i64>(0)))
            .unwrap_or(0)
            > 0;
        if !has_display_name {
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN display_name TEXT;")
                .map_err(|e| format!("migration error (display_name): {e}"))?;
        }

        // Comparisons table for side-by-side model comparison results (CPO-17).
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS comparisons (
                id               TEXT PRIMARY KEY,
                session_id       TEXT    NOT NULL,
                input_json       TEXT    NOT NULL,
                left_model_id    TEXT    NOT NULL,
                right_model_id   TEXT    NOT NULL,
                left_message_id  TEXT,
                right_message_id TEXT,
                created_at       INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_comparisons_session
                ON comparisons(session_id, created_at);
            ",
        )
        .map_err(|e| format!("migration error (comparisons): {e}"))?;

        // Enable foreign key enforcement.
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("pragma error: {e}"))?;

        Ok(())
    }

    // -- Session CRUD -------------------------------------------------------

    /// Create a new chat session associated with the given model.
    pub fn create_session(
        &self,
        model_id: &str,
        model_name: &str,
    ) -> Result<Session, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;
        conn.execute(
            "INSERT INTO sessions (id, model_id, model_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, model_id, model_name, now, now],
        )
        .map_err(|e| format!("insert session error: {e}"))?;

        Ok(Session {
            id,
            model_id: model_id.to_string(),
            model_name: model_name.to_string(),
            created_at: now,
            updated_at: now,
            preview: String::new(),
            message_count: 0,
        })
    }

    /// List all sessions ordered by most-recently-updated first.
    ///
    /// Each session is enriched with `message_count` and a `preview` derived
    /// from the first user message (if any).
    pub fn list_sessions(&self) -> Result<Vec<Session>, String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT
                     s.id,
                     s.model_id,
                     s.model_name,
                     s.created_at,
                     s.updated_at,
                     (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS msg_count,
                     (SELECT m.content_json
                        FROM messages m
                       WHERE m.session_id = s.id AND m.role = 'User'
                       ORDER BY m.timestamp ASC
                       LIMIT 1) AS first_user_msg,
                     s.display_name
                 FROM sessions s
                 ORDER BY s.updated_at DESC",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let content_json: Option<String> = row.get(6)?;
                let display_name: Option<String> = row.get(7)?;

                let preview = if let Some(name) = display_name {
                    name
                } else {
                    content_json
                        .and_then(|json| {
                            serde_json::from_str::<MessageContent>(&json)
                                .ok()
                                .map(|mc| mc.preview(80))
                        })
                        .unwrap_or_default()
                };

                Ok(Session {
                    id: row.get(0)?,
                    model_id: row.get(1)?,
                    model_name: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    message_count: row.get::<_, i64>(5)? as usize,
                    preview,
                })
            })
            .map_err(|e| format!("query error: {e}"))?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| format!("row error: {e}"))?);
        }
        Ok(sessions)
    }

    /// Get a single session by ID (without messages).
    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>, String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT
                     s.id,
                     s.model_id,
                     s.model_name,
                     s.created_at,
                     s.updated_at,
                     (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS msg_count,
                     (SELECT m.content_json
                        FROM messages m
                       WHERE m.session_id = s.id AND m.role = 'User'
                       ORDER BY m.timestamp ASC
                       LIMIT 1) AS first_user_msg,
                     s.display_name
                 FROM sessions s
                 WHERE s.id = ?1",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let session = stmt
            .query_row(params![session_id], |row| {
                let content_json: Option<String> = row.get(6)?;
                let display_name: Option<String> = row.get(7)?;

                let preview = if let Some(name) = display_name {
                    name
                } else {
                    content_json
                        .and_then(|json| {
                            serde_json::from_str::<MessageContent>(&json)
                                .ok()
                                .map(|mc| mc.preview(80))
                        })
                        .unwrap_or_default()
                };

                Ok(Session {
                    id: row.get(0)?,
                    model_id: row.get(1)?,
                    model_name: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    message_count: row.get::<_, i64>(5)? as usize,
                    preview,
                })
            })
            .optional()
            .map_err(|e| format!("query error: {e}"))?;

        Ok(session)
    }

    /// Delete a session and all of its messages.
    pub fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        // Messages are cascaded by FK, but delete explicitly for clarity /
        // in case FK enforcement is disabled.
        conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete messages error: {e}"))?;

        conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
            .map_err(|e| format!("delete session error: {e}"))?;

        Ok(())
    }

    /// Rename a session by updating its display name (stored in `display_name`
    /// column, surfaced as the `preview` field) and refreshing `updated_at`.
    pub fn rename_session(&self, session_id: &str, new_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;
        let now = chrono::Utc::now().timestamp();

        let rows = conn
            .execute(
                "UPDATE sessions SET display_name = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_name, now, session_id],
            )
            .map_err(|e| format!("rename session error: {e}"))?;

        if rows == 0 {
            return Err(format!("session not found: {session_id}"));
        }

        Ok(())
    }

    // -- Message CRUD -------------------------------------------------------

    /// Retrieve all messages for a session, ordered by timestamp.
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>, String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, role, content_json, timestamp, inference_ms
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                let role_str: String = row.get(1)?;
                let content_json: String = row.get(2)?;
                let inference_ms: Option<u64> = row.get(4)?;

                let role = match role_str.as_str() {
                    "User" => MessageRole::User,
                    "Model" => MessageRole::Model,
                    "System" => MessageRole::System,
                    _ => MessageRole::System,
                };

                let content: MessageContent = serde_json::from_str(&content_json)
                    .unwrap_or(MessageContent::Text("[deserialization error]".into()));

                Ok(ChatMessage {
                    id: row.get(0)?,
                    role,
                    content,
                    timestamp: row.get(3)?,
                    inference_ms,
                })
            })
            .map_err(|e| format!("query error: {e}"))?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| format!("row error: {e}"))?);
        }
        Ok(messages)
    }

    /// Insert a message into the store and update the parent session's
    /// `updated_at` timestamp.
    pub fn add_message(
        &self,
        session_id: &str,
        msg: &ChatMessage,
    ) -> Result<(), String> {
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e| format!("serialize error: {e}"))?;

        let role_str = match msg.role {
            MessageRole::User => "User",
            MessageRole::Model => "Model",
            MessageRole::System => "System",
        };

        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        conn.execute(
            "INSERT INTO messages (id, session_id, role, content_json, timestamp, inference_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                msg.id,
                session_id,
                role_str,
                content_json,
                msg.timestamp,
                msg.inference_ms,
            ],
        )
        .map_err(|e| format!("insert message error: {e}"))?;

        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )
        .map_err(|e| format!("update session error: {e}"))?;

        Ok(())
    }

    // -- Comparison CRUD (CPO-17) -------------------------------------------

    /// Persist a comparison record linking an input to two model responses.
    pub fn save_comparison(
        &self,
        session_id: &str,
        input_json: &str,
        left_model_id: &str,
        right_model_id: &str,
        left_message_id: Option<&str>,
        right_message_id: Option<&str>,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;
        conn.execute(
            "INSERT INTO comparisons (id, session_id, input_json, left_model_id, right_model_id, left_message_id, right_message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                session_id,
                input_json,
                left_model_id,
                right_model_id,
                left_message_id,
                right_message_id,
                now,
            ],
        )
        .map_err(|e| format!("insert comparison error: {e}"))?;

        Ok(id)
    }

    /// Retrieve all comparisons for a given session, ordered by creation time.
    pub fn get_comparisons(
        &self,
        session_id: &str,
    ) -> Result<Vec<ComparisonRecord>, String> {
        let conn = self.conn.lock().map_err(|e| format!("lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, input_json, left_model_id, right_model_id,
                        left_message_id, right_message_id, created_at
                 FROM comparisons
                 WHERE session_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(ComparisonRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    input_json: row.get(2)?,
                    left_model_id: row.get(3)?,
                    right_model_id: row.get(4)?,
                    left_message_id: row.get(5)?,
                    right_message_id: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("query error: {e}"))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| format!("row error: {e}"))?);
        }
        Ok(records)
    }
}

/// A persisted comparison record from the `comparisons` table.
#[derive(Clone, Debug)]
pub struct ComparisonRecord {
    pub id: String,
    pub session_id: String,
    pub input_json: String,
    pub left_model_id: String,
    pub right_model_id: String,
    pub left_message_id: Option<String>,
    pub right_message_id: Option<String>,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// Async wrappers (convenience for Tokio callers)
// ---------------------------------------------------------------------------
//
// These mirror every public method above but take `self: &Arc<Self>` so the
// `Arc` can be cloned into the `spawn_blocking` closure. String parameters are
// owned to avoid lifetime issues across the blocking boundary.
//
// The async wrappers use tokio which is only available under the `ssr` feature.
// Since this entire module is already `#[cfg(feature = "ssr")]`-gated via
// mod.rs, no additional gating is needed.

impl SessionStore {
    pub async fn create_session_async(
        self: &Arc<Self>,
        model_id: String,
        model_name: String,
    ) -> Result<Session, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.create_session(&model_id, &model_name))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn list_sessions_async(self: &Arc<Self>) -> Result<Vec<Session>, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_sessions())
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn get_session_async(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<Option<Session>, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_session(&session_id))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn rename_session_async(
        self: &Arc<Self>,
        session_id: String,
        new_name: String,
    ) -> Result<(), String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.rename_session(&session_id, &new_name))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn delete_session_async(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<(), String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.delete_session(&session_id))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn get_messages_async(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<Vec<ChatMessage>, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_messages(&session_id))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn add_message_async(
        self: &Arc<Self>,
        session_id: String,
        msg: ChatMessage,
    ) -> Result<(), String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.add_message(&session_id, &msg))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn save_comparison_async(
        self: &Arc<Self>,
        session_id: String,
        input_json: String,
        left_model_id: String,
        right_model_id: String,
        left_message_id: Option<String>,
        right_message_id: Option<String>,
    ) -> Result<String, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            store.save_comparison(
                &session_id,
                &input_json,
                &left_model_id,
                &right_model_id,
                left_message_id.as_deref(),
                right_message_id.as_deref(),
            )
        })
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?
    }

    pub async fn get_comparisons_async(
        self: &Arc<Self>,
        session_id: String,
    ) -> Result<Vec<ComparisonRecord>, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_comparisons(&session_id))
            .await
            .map_err(|e| format!("spawn_blocking error: {e}"))?
    }
}

// ---------------------------------------------------------------------------
// Helpers — rusqlite optional row
// ---------------------------------------------------------------------------

/// Extension trait so we can use `.optional()` on `query_row` results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
