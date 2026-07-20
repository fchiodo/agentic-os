use rusqlite::params;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::control_models::{AuditChainStatus, TraceEntry};
use crate::db::Db;
use crate::error::AppResult;

const GENESIS_HASH_LEN: usize = 64;

fn genesis_hash() -> String {
    "0".repeat(GENESIS_HASH_LEN)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[allow(clippy::too_many_arguments)]
fn compute_hash(
    prev_hash: &str,
    run_id: &str,
    task_id: &str,
    ts: &str,
    kind: &str,
    summary: &str,
    detail_str: &str,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(run_id.as_bytes());
    hasher.update(task_id.as_bytes());
    hasher.update(ts.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(summary.as_bytes());
    hasher.update(detail_str.as_bytes());
    hasher.update(tokens.unwrap_or_default().to_string().as_bytes());
    hasher.update(cost_usd.unwrap_or_default().to_string().as_bytes());
    hex_encode(hasher.finalize().as_slice())
}

/// Appends one append-only, hash-chained audit row. The chain is global
/// (across every run, not scoped per task) so reordering across runs is
/// also tamper-evident. See ARCHITECTURE.md section 9.
#[allow(clippy::too_many_arguments)]
pub fn append_row(
    db: &Db,
    run_id: &str,
    task_id: &str,
    kind: &str,
    summary: &str,
    detail: &Value,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
) -> AppResult<()> {
    db.with_conn(|conn| {
        let prev_hash: String = conn
            .query_row("SELECT hash FROM audit ORDER BY id DESC LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| genesis_hash());

        let ts = chrono::Utc::now().to_rfc3339();
        let detail_str = serde_json::to_string(detail)?;
        let hash = compute_hash(
            &prev_hash, run_id, task_id, &ts, kind, summary, &detail_str, tokens, cost_usd,
        );

        conn.execute(
            "INSERT INTO audit (run_id, task_id, ts, kind, summary, detail, tokens, cost_usd, prev_hash, hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![run_id, task_id, ts, kind, summary, detail_str, tokens, cost_usd, prev_hash, hash],
        )?;
        Ok(())
    })
}

pub fn verify_chain(db: &Db) -> AppResult<AuditChainStatus> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, run_id, task_id, ts, kind, summary, detail, tokens, cost_usd, prev_hash, hash
             FROM audit ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<f64>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;

        let mut expected_prev = genesis_hash();
        let mut checked = 0i64;

        for row in rows {
            let (id, run_id, task_id, ts, kind, summary, detail_str, tokens, cost_usd, prev_hash, hash) =
                row?;
            checked += 1;
            let task_id_str = task_id.unwrap_or_default();

            if prev_hash != expected_prev {
                return Ok(AuditChainStatus {
                    ok: false,
                    checked_rows: checked,
                    broken_at: Some(id.to_string()),
                });
            }

            let recomputed = compute_hash(
                &prev_hash, &run_id, &task_id_str, &ts, &kind, &summary, &detail_str, tokens,
                cost_usd,
            );

            if recomputed != hash {
                return Ok(AuditChainStatus {
                    ok: false,
                    checked_rows: checked,
                    broken_at: Some(id.to_string()),
                });
            }

            expected_prev = hash;
        }

        Ok(AuditChainStatus {
            ok: true,
            checked_rows: checked,
            broken_at: None,
        })
    })
}

pub fn read_trace(db: &Db, run_id: &str) -> AppResult<Vec<TraceEntry>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, ts, kind, summary, detail, tokens, cost_usd FROM audit
             WHERE run_id = ?1 ORDER BY id ASC",
        )?;
        let entries = stmt
            .query_map(params![run_id], |row| {
                let detail_str: String = row.get(4)?;
                let detail: Value =
                    serde_json::from_str(&detail_str).unwrap_or(Value::Null);
                Ok(TraceEntry {
                    run_id: run_id.to_string(),
                    seq: row.get(0)?,
                    ts: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get(3)?,
                    detail,
                    tokens: row.get(5)?,
                    cost_usd: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    })
}
