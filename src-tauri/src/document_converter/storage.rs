use rusqlite::{params, OptionalExtension, Row};

use crate::db::Db;

use super::errors::{ConverterError, ConverterResult};
use super::types::{ConversionJob, ConversionOptions};

const ACTIVE_STATUSES: &[&str] = &[
    "queued",
    "preparing",
    "rendering",
    "ocr",
    "reconstructing",
    "writing",
];

pub fn ensure_tables(db: &Db) -> ConverterResult<()> {
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS document_conversion_jobs (
                id TEXT PRIMARY KEY,
                source_name TEXT NOT NULL,
                source_path TEXT NOT NULL,
                source_hash TEXT NOT NULL,
                source_size_bytes INTEGER NOT NULL,
                destination_root TEXT NOT NULL,
                output_path TEXT,
                markdown_path TEXT,
                json_path TEXT,
                engine TEXT NOT NULL,
                engine_version TEXT NOT NULL,
                model_version TEXT NOT NULL,
                status TEXT NOT NULL,
                stage TEXT NOT NULL,
                stage_label TEXT,
                total_pages INTEGER,
                processed_pages INTEGER NOT NULL DEFAULT 0,
                pages_digital INTEGER NOT NULL DEFAULT 0,
                pages_ocr INTEGER NOT NULL DEFAULT 0,
                asset_count INTEGER NOT NULL DEFAULT 0,
                processing_mode TEXT NOT NULL,
                options_json TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                warnings_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                error_code TEXT,
                error_message TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_document_conversion_created
                ON document_conversion_jobs(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_document_conversion_fingerprint
                ON document_conversion_jobs(fingerprint, status);
            CREATE INDEX IF NOT EXISTS idx_document_conversion_status
                ON document_conversion_jobs(status);
            "#,
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn insert(db: &Db, job: &ConversionJob) -> ConverterResult<()> {
    let options = serde_json::to_string(&job.options)?;
    let warnings = serde_json::to_string(&job.warnings)?;
    db.with_conn(|conn| {
        conn.execute(
            r#"INSERT INTO document_conversion_jobs (
                id, source_name, source_path, source_hash, source_size_bytes,
                destination_root, output_path, markdown_path, json_path,
                engine, engine_version, model_version, status, stage, stage_label,
                total_pages, processed_pages, pages_digital, pages_ocr, asset_count,
                processing_mode, options_json, fingerprint, warnings_json,
                created_at, started_at, completed_at, error_code, error_message
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
                ?27, ?28, ?29
            )"#,
            params![
                job.id,
                job.source_name,
                job.source_path,
                job.source_hash,
                job.source_size_bytes as i64,
                job.destination_root,
                job.output_path,
                job.markdown_path,
                job.json_path,
                job.engine,
                job.engine_version,
                job.model_version,
                job.status,
                job.stage,
                job.stage_label,
                job.total_pages,
                job.processed_pages,
                job.pages_digital,
                job.pages_ocr,
                job.asset_count,
                job.processing_mode,
                options,
                job.fingerprint,
                warnings,
                job.created_at,
                job.started_at,
                job.completed_at,
                job.error_code,
                job.error_message,
            ],
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn save(db: &Db, job: &ConversionJob) -> ConverterResult<()> {
    let options = serde_json::to_string(&job.options)?;
    let warnings = serde_json::to_string(&job.warnings)?;
    db.with_conn(|conn| {
        let changed = conn.execute(
            r#"UPDATE document_conversion_jobs SET
                output_path=?2, markdown_path=?3, json_path=?4, engine=?5,
                engine_version=?6, model_version=?7, status=?8, stage=?9,
                stage_label=?10, total_pages=?11, processed_pages=?12,
                pages_digital=?13, pages_ocr=?14, asset_count=?15,
                processing_mode=?16, options_json=?17, fingerprint=?18,
                warnings_json=?19, started_at=?20, completed_at=?21,
                error_code=?22, error_message=?23
            WHERE id=?1"#,
            params![
                job.id,
                job.output_path,
                job.markdown_path,
                job.json_path,
                job.engine,
                job.engine_version,
                job.model_version,
                job.status,
                job.stage,
                job.stage_label,
                job.total_pages,
                job.processed_pages,
                job.pages_digital,
                job.pages_ocr,
                job.asset_count,
                job.processing_mode,
                options,
                job.fingerprint,
                warnings,
                job.started_at,
                job.completed_at,
                job.error_code,
                job.error_message,
            ],
        )?;
        if changed == 0 {
            return Err(crate::error::AppError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            ));
        }
        Ok(())
    })?;
    Ok(())
}

pub fn get(db: &Db, id: &str) -> ConverterResult<ConversionJob> {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT * FROM document_conversion_jobs WHERE id=?1",
            [id],
            row_to_job,
        )?)
    })
    .map_err(Into::into)
}

pub fn list(db: &Db, limit: u32) -> ConverterResult<Vec<ConversionJob>> {
    db.with_conn(|conn| {
        let mut statement = conn
            .prepare("SELECT * FROM document_conversion_jobs ORDER BY created_at DESC LIMIT ?1")?;
        let rows = statement.query_map([limit.clamp(1, 200)], row_to_job)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
    .map_err(Into::into)
}

pub fn find_completed_fingerprint(
    db: &Db,
    fingerprint: &str,
) -> ConverterResult<Option<ConversionJob>> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT * FROM document_conversion_jobs WHERE fingerprint=?1 AND status='completed' ORDER BY completed_at DESC LIMIT 1",
            [fingerprint],
            row_to_job,
        )
        .optional()
        .map_err(Into::into)
    })
    .map_err(Into::into)
}

pub fn delete_history(db: &Db, id: &str) -> ConverterResult<()> {
    let job = get(db, id)?;
    if ACTIVE_STATUSES.contains(&job.status.as_str()) {
        return Err(ConverterError::new(
            "JOB_ACTIVE",
            "Cancel the conversion before removing it from history",
        ));
    }
    db.with_conn(|conn| {
        conn.execute("DELETE FROM document_conversion_jobs WHERE id=?1", [id])?;
        Ok(())
    })?;
    Ok(())
}

pub fn recover_interrupted(db: &Db) -> ConverterResult<u32> {
    let now = chrono::Utc::now().to_rfc3339();
    let recovered = db.with_conn(|conn| {
        let count = conn.execute(
            "UPDATE document_conversion_jobs SET status='failed', stage='failed', stage_label='Interrupted by application restart', completed_at=?1, error_code='CONVERSION_INTERRUPTED', error_message='Conversion was interrupted when Agentic OS closed' WHERE status IN ('preparing','rendering','ocr','reconstructing','writing')",
            [now],
        )?;
        conn.execute(
            "UPDATE document_conversion_jobs SET status='cancelled', stage='cancelled', stage_label='Cancelled during application restart', completed_at=?1, error_code='CONVERSION_CANCELLED', error_message='Queued conversion was cancelled when Agentic OS restarted' WHERE status='queued'",
            [chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(count as u32)
    })?;
    Ok(recovered)
}

pub fn count_active(db: &Db) -> ConverterResult<(u32, u32)> {
    db.with_conn(|conn| {
        let active: u32 = conn.query_row(
            "SELECT COUNT(*) FROM document_conversion_jobs WHERE status IN ('preparing','rendering','ocr','reconstructing','writing')",
            [],
            |row| row.get(0),
        )?;
        let queued: u32 = conn.query_row(
            "SELECT COUNT(*) FROM document_conversion_jobs WHERE status='queued'",
            [],
            |row| row.get(0),
        )?;
        Ok((active, queued))
    })
    .map_err(Into::into)
}

pub fn can_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("queued", "preparing")
            | ("queued", "cancelled")
            | ("preparing", "rendering")
            | ("preparing", "ocr")
            | ("preparing", "reconstructing")
            | ("preparing", "cancelled")
            | ("preparing", "failed")
            | ("rendering", "ocr")
            | ("rendering", "reconstructing")
            | ("rendering", "cancelled")
            | ("rendering", "failed")
            | ("ocr", "rendering")
            | ("ocr", "reconstructing")
            | ("ocr", "cancelled")
            | ("ocr", "failed")
            | ("reconstructing", "writing")
            | ("reconstructing", "cancelled")
            | ("reconstructing", "failed")
            | ("writing", "completed")
            | ("writing", "cancelled")
            | ("writing", "failed")
            | ("failed", "queued")
            | ("cancelled", "queued")
    ) || from == to
}

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<ConversionJob> {
    let options_json: String = row.get("options_json")?;
    let warnings_json: String = row.get("warnings_json")?;
    Ok(ConversionJob {
        id: row.get("id")?,
        source_name: row.get("source_name")?,
        source_path: row.get("source_path")?,
        source_hash: row.get("source_hash")?,
        source_size_bytes: row.get::<_, i64>("source_size_bytes")?.max(0) as u64,
        destination_root: row.get("destination_root")?,
        output_path: row.get("output_path")?,
        markdown_path: row.get("markdown_path")?,
        json_path: row.get("json_path")?,
        engine: row.get("engine")?,
        engine_version: row.get("engine_version")?,
        model_version: row.get("model_version")?,
        status: row.get("status")?,
        stage: row.get("stage")?,
        stage_label: row.get("stage_label")?,
        total_pages: row.get("total_pages")?,
        processed_pages: row.get("processed_pages")?,
        pages_digital: row.get("pages_digital")?,
        pages_ocr: row.get("pages_ocr")?,
        asset_count: row.get("asset_count")?,
        processing_mode: row.get("processing_mode")?,
        options: serde_json::from_str(&options_json)
            .unwrap_or_else(|_| ConversionOptions::default()),
        fingerprint: row.get("fingerprint")?,
        warnings: serde_json::from_str(&warnings_json).unwrap_or_default(),
        created_at: row.get("created_at")?,
        started_at: row.get("started_at")?,
        completed_at: row.get("completed_at")?,
        error_code: row.get("error_code")?,
        error_message: row.get("error_message")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_state_machine() {
        assert!(can_transition("queued", "preparing"));
        assert!(can_transition("ocr", "reconstructing"));
        assert!(!can_transition("completed", "ocr"));
    }
}
