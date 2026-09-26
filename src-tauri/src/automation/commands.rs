use tauri::{AppHandle, State};

use super::config::AutomationConfig;
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
    state.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn send_test_notification(notifier: State<'_, Notifier>) -> Result<(), String> {
    notifier.send_test(
        "Solar Hub",
        "Test notification — native banners are working.",
    );
    Ok(())
}
