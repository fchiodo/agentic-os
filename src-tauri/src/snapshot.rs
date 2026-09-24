use std::time::{SystemTime, UNIX_EPOCH};

use crate::discovery;
use crate::error::AppResult;
use crate::models::DashboardSnapshot;

pub fn load_snapshot() -> AppResult<DashboardSnapshot> {
    let discovery = discovery::discover()?;
    Ok(DashboardSnapshot {
        generated_at: now_millis(),
        catalog: discovery.catalog,
        sources: discovery.sources,
        runtime: discovery.runtime,
    })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}
