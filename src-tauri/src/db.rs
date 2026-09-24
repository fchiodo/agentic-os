use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::AppResult;

/// App-owned SQLite database for memory, document processing, and historical audit data.
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
                    status TEXT NOT NULL,
                    origin_kind TEXT NOT NULL,
                    ontology_category_id TEXT,
                    cost_usd REAL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
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

                UPDATE tasks
                SET status = 'cancelled'
                WHERE status = 'waiting_for_approval';

                DROP TABLE IF EXISTS approvals;
                DROP TABLE IF EXISTS task_steps;
                DROP TABLE IF EXISTS events;
                DROP TABLE IF EXISTS artifacts;
                "#,
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Db;

    #[test]
    fn migration_removes_retired_runner_and_approval_tables() {
        let path = std::env::temp_dir().join(format!(
            "agentic-os-retired-runner-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&path).unwrap();
        db.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE approvals (id TEXT PRIMARY KEY);
                CREATE TABLE task_steps (task_id TEXT);
                CREATE TABLE events (task_id TEXT);
                CREATE TABLE artifacts (task_id TEXT);
                INSERT INTO tasks (
                    id, title, goal, domain, status, origin_kind, created_at, updated_at
                ) VALUES (
                    'legacy-task', 'Legacy task', 'Legacy goal', 'work',
                    'waiting_for_approval', 'manual', '2026-09-24T00:00:00Z',
                    '2026-09-24T00:00:00Z'
                );
                "#,
            )?;
            Ok(())
        })
        .unwrap();
        drop(db);

        let db = Db::open(&path).unwrap();
        db.with_conn(|conn| {
            for table in ["approvals", "task_steps", "events", "artifacts"] {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 0, "{table} should be removed");
            }

            let status: String = conn.query_row(
                "SELECT status FROM tasks WHERE id = 'legacy-task'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(status, "cancelled");
            Ok(())
        })
        .unwrap();
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

// Memory tables live in the same system-of-record DB and are migrated during
// Db::open via memory::index::ensure_tables(). The function remains
// idempotent because commands and tests may call it defensively.
