use crate::models::DashboardSnapshot;
use crate::snapshot;

#[tauri::command]
pub fn get_app_snapshot() -> Result<DashboardSnapshot, String> {
    snapshot::load_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn refresh_app_snapshot() -> Result<DashboardSnapshot, String> {
    snapshot::load_snapshot().map_err(|error| error.to_string())
}
