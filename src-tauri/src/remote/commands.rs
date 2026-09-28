use tauri::{AppHandle, State};

use super::{RemoteState, RemoteStatus};

#[tauri::command]
pub async fn get_remote_status(state: State<'_, RemoteState>) -> Result<RemoteStatus, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn set_remote_enabled(
    app: AppHandle,
    state: State<'_, RemoteState>,
    enabled: bool,
) -> Result<RemoteStatus, String> {
    state.set_enabled(&app, enabled).await
}

#[tauri::command]
pub async fn regenerate_remote_pin(app: AppHandle, state: State<'_, RemoteState>) -> Result<RemoteStatus, String> {
    state.regenerate_pin(&app).await
}
