use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::AppResult;

/// App-owned SQLite database (tasks, events, approvals, audit). Distinct from
/// the read-only Codex databases discovery.rs and snapshot.rs read from.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        crate::memory::index::ensure_tables(&db)?;
        Ok(db)
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        f(&conn)
    }

    fn migrate(&self) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    goal TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    agent_id TEXT,
                    harness TEXT NOT NULL,
                    status TEXT NOT NULL,
                    origin_kind TEXT NOT NULL,
                    ontology_category_id TEXT,
                    plan_version INTEGER NOT NULL DEFAULT 1,
                    current_step INTEGER NOT NULL DEFAULT 0,
                    step_count INTEGER NOT NULL DEFAULT 0,
                    cost_tokens INTEGER NOT NULL DEFAULT 0,
                    cost_usd REAL,
                    pending_approval_id TEXT,
                    thread_id TEXT,
                    sandbox_mode TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    risk_level TEXT NOT NULL,
                    failure_reason TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_steps (
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    step_index INTEGER NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    PRIMARY KEY (task_id, step_index)
                );

                CREATE TABLE IF NOT EXISTS events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    ts TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    payload TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_events_task ON events(task_id, seq);

                CREATE TABLE IF NOT EXISTS approvals (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    domain TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    action_summary TEXT NOT NULL,
                    risk_level TEXT NOT NULL,
                    preview_kind TEXT,
                    preview_content TEXT,
                    requested_at TEXT NOT NULL,
                    status TEXT NOT NULL,
                    decided_at TEXT,
                    note TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_approvals_status ON approvals(status);

                CREATE TABLE IF NOT EXISTS artifacts (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    label TEXT NOT NULL,
                    path TEXT NOT NULL,
                    kind TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id TEXT NOT NULL,
                    task_id TEXT,
                    ts TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    tokens INTEGER,
                    cost_usd REAL,
                    prev_hash TEXT NOT NULL,
                    hash TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_audit_run ON audit(run_id);
                "#,
            )?;
            Ok(())
        })
    }
}
// Memory tables live in the same system-of-record DB and are migrated during
// Db::open via memory::index::ensure_tables(). The function remains
// idempotent because commands and tests may call it defensively.
