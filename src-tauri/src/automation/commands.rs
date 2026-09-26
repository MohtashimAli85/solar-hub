use tauri::{AppHandle, State};

use super::config::AutomationConfig;
use super::engine::evaluate;
use super::{AutomationState, AutomationStatus};
use crate::battery::BatteryState;
use crate::inverter::InverterState;
use crate::notifications::Notifier;

#[tauri::command]
pub async fn get_automation_status(
    state: State<'_, AutomationState>,
) -> Result<AutomationStatus, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn get_automation_config(
    state: State<'_, AutomationState>,
) -> Result<AutomationConfig, String> {
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
pub async fn force_automation_check(
    state: State<'_, AutomationState>,
    inverter_state: State<'_, InverterState>,
    battery: State<'_, BatteryState>,
    notifier: State<'_, Notifier>,
) -> Result<AutomationStatus, String> {
    if !state.config().await.enabled {
        return Ok(state.status().await);
    }
    let client = inverter_state.solar_client();
    state
        .set_status(|status| status.last_check = Some(chrono::Local::now().to_rfc3339()))
        .await;
    match evaluate(&state, &client, &battery, &notifier).await {
        Ok(()) => {}
        Err(error) => state.log(format!("Automation check failed: {error}")).await,
    }
    Ok(state.status().await)
}

#[tauri::command]
pub async fn dismiss_automation_warning(
    state: State<'_, AutomationState>,
) -> Result<AutomationStatus, String> {
    state.set_status(|status| status.warning = None).await;
    Ok(state.status().await)
}

#[tauri::command]
pub async fn send_test_notification(notifier: State<'_, Notifier>) -> Result<(), String> {
    notifier.send_test(
        "Solar Hub",
        "Test notification — native banners are working.",
    );
    Ok(())
}