use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::config::AutomationConfig;
use super::history::DecisionRow;
use super::insights::AutomationInsights;
use super::{AutomationState, AutomationStatus};
use crate::notifications::Notifier;

#[tauri::command]
pub async fn get_automation_status(state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn get_automation_config(state: State<'_, AutomationState>) -> Result<AutomationConfig, String> {
    Ok(state.config().await)
}

#[tauri::command]
pub async fn update_automation_config(
    app: AppHandle,
    state: State<'_, AutomationState>,
    config: AutomationConfig,
) -> Result<AutomationConfig, String> {
    state.apply_config(&app, config).await
}

#[tauri::command]
pub async fn force_automation_check(state: State<'_, AutomationState>) -> Result<(), String> {
    state.force_check.store(true, std::sync::atomic::Ordering::SeqCst);
    state.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn get_automation_insights(state: State<'_, AutomationState>) -> Result<AutomationInsights, String> {
    Ok(state.insights().await)
}

#[tauri::command]
pub async fn get_automation_decisions(
    state: State<'_, AutomationState>,
    limit: Option<usize>,
) -> Result<Vec<DecisionRow>, String> {
    let today = chrono::Local::now().date_naive();
    Ok(state.history.recent_decisions(today, limit.unwrap_or(20).min(200)))
}

#[tauri::command]
pub async fn open_records_folder(app: AppHandle, state: State<'_, AutomationState>) -> Result<(), String> {
    let root = state.history.root();
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(root.display().to_string(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn send_test_notification(notifier: State<'_, Notifier>) -> Result<(), String> {
    notifier.send_test(
        "Solar Hub",
        "Test notification — native banners are working.",
    );
    Ok(())
}
