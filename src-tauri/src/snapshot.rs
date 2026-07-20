use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use crate::discovery;
use crate::error::AppResult;
use crate::models::{
    ActivitySection, DashboardSnapshot, JobSummary, SourceDescriptor, SourceStatus, ThreadSummary,
    UsagePoint, UsageSection, WorkspaceUsage,
};

pub fn load_snapshot() -> AppResult<DashboardSnapshot> {
    let discovery = discovery::discover()?;
    let codex_home = dirs::home_dir()
        .ok_or(crate::error::AppError::MissingHomeDirectory)?
        .join(".codex");
    let state_path = codex_home.join("state_5.sqlite");
    let logs_path = codex_home.join("logs_2.sqlite");

    let mut sources = discovery.sources;
    register_database_source(
        &mut sources,
        "codex-state",
        "State database",
        &state_path,
        &codex_home,
    );
    register_database_source(
        &mut sources,
        "codex-logs",
        "Logs database",
        &logs_path,
        &codex_home,
    );

    let activity = load_activity(&state_path)?;
    let usage = load_usage(&state_path, &logs_path)?;

    Ok(DashboardSnapshot {
        generated_at: now_millis(),
        catalog: discovery.catalog,
        activity,
        usage,
        sources,
        runtime: discovery.runtime,
    })
}

fn load_activity(state_path: &Path) -> AppResult<ActivitySection> {
    let Some(connection) = open_readonly(state_path)? else {
        return Ok(ActivitySection {
            recent_threads: Vec::new(),
            recent_jobs: Vec::new(),
        });
    };

    let recent_threads = if table_exists(&connection, "threads")? {
        let mut statement = connection.prepare(
      "SELECT id, title, cwd, COALESCE(updated_at_ms, updated_at * 1000), tokens_used, model, model_provider
       FROM threads
       ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC
       LIMIT 8",
    )?;

        let rows = statement
            .query_map([], |row| {
                let model = row.get::<_, Option<String>>(5)?;
                Ok(ThreadSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    cwd: row.get(2)?,
                    updated_at: row.get(3)?,
                    tokens_used: row.get(4)?,
                    model: model.filter(|value| !value.is_empty()),
                    provider: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows
    } else {
        Vec::new()
    };

    let recent_jobs = if table_exists(&connection, "agent_jobs")? {
        let mut statement = connection.prepare(
      "SELECT id, name, status, input_csv_path, output_csv_path, updated_at * 1000, max_runtime_seconds
       FROM agent_jobs
       ORDER BY updated_at DESC
       LIMIT 6",
    )?;

        let rows = statement
            .query_map([], |row| {
                Ok(JobSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    input_path: row.get(3)?,
                    output_path: row.get(4)?,
                    updated_at: row.get(5)?,
                    max_runtime_seconds: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows
    } else {
        Vec::new()
    };

    Ok(ActivitySection {
        recent_threads,
        recent_jobs,
    })
}

fn load_usage(state_path: &Path, logs_path: &Path) -> AppResult<UsageSection> {
    let mut usage = UsageSection {
        total_tokens: 0,
        tracked_threads: 0,
        active_threads: 0,
        distinct_workspaces: 0,
        log_entries_24h: 0,
        trend: Vec::new(),
        top_workspaces: Vec::new(),
    };

    if let Some(connection) = open_readonly(state_path)? {
        if table_exists(&connection, "threads")? {
            usage.total_tokens = query_single_i64(
                &connection,
                "SELECT COALESCE(SUM(tokens_used), 0) FROM threads",
            )?;
            usage.tracked_threads = query_single_i64(&connection, "SELECT COUNT(*) FROM threads")?;
            usage.active_threads = query_single_i64(
                &connection,
                "SELECT COUNT(*) FROM threads WHERE archived = 0",
            )?;
            usage.distinct_workspaces =
                query_single_i64(&connection, "SELECT COUNT(DISTINCT cwd) FROM threads")?;

            let mut workspace_statement = connection.prepare(
        "SELECT cwd, COUNT(*) AS thread_count, COALESCE(SUM(tokens_used), 0) AS token_total,
                MAX(COALESCE(updated_at_ms, updated_at * 1000)) AS last_updated_at
         FROM threads
         GROUP BY cwd
         ORDER BY token_total DESC, last_updated_at DESC
         LIMIT 6",
      )?;

            usage.top_workspaces = workspace_statement
                .query_map([], |row| {
                    Ok(WorkspaceUsage {
                        cwd: row.get(0)?,
                        thread_count: row.get(1)?,
                        token_total: row.get(2)?,
                        last_updated_at: row.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut trend_statement = connection.prepare(
                "SELECT strftime('%m-%d', datetime(updated_at, 'unixepoch')) AS day,
                COALESCE(SUM(tokens_used), 0) AS token_total
         FROM threads
         WHERE updated_at >= strftime('%s', 'now', '-6 days')
         GROUP BY date(updated_at, 'unixepoch')
         ORDER BY date(updated_at, 'unixepoch') ASC",
            )?;

            usage.trend = trend_statement
                .query_map([], |row| {
                    Ok(UsagePoint {
                        day: row.get(0)?,
                        token_total: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
        }
    }

    if let Some(connection) = open_readonly(logs_path)? {
        if table_exists(&connection, "logs")? {
            let lower_bound = (now_millis() / 1000) - 86_400;
            let mut statement = connection.prepare("SELECT COUNT(*) FROM logs WHERE ts >= ?1")?;
            usage.log_entries_24h = statement.query_row([lower_bound], |row| row.get(0))?;
        }
    }

    Ok(usage)
}

fn open_readonly(path: &Path) -> AppResult<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }

    Ok(Some(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?))
}

fn query_single_i64(connection: &Connection, sql: &str) -> AppResult<i64> {
    let mut statement = connection.prepare(sql)?;
    Ok(statement.query_row([], |row| row.get(0))?)
}

fn register_database_source(
    sources: &mut Vec<SourceDescriptor>,
    id: &str,
    label: &str,
    path: &Path,
    codex_home: &Path,
) {
    let home_dir = codex_home.parent().unwrap_or(codex_home);
    sources.push(SourceDescriptor {
        id: id.into(),
        label: label.into(),
        kind: "database".into(),
        path: path
            .strip_prefix(home_dir)
            .map(|relative| format!("~/{}", relative.display()))
            .unwrap_or_else(|_| path.display().to_string()),
        status: if path.exists() {
            SourceStatus::Available
        } else {
            SourceStatus::Missing
        },
    });
}

fn table_exists(connection: &Connection, table: &str) -> AppResult<bool> {
    let mut statement = connection
        .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    let count = statement.query_row([table], |row| row.get::<_, i64>(0))?;
    Ok(count > 0)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}
