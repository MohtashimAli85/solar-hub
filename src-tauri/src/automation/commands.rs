use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::config::AutomationConfig;
use crate::geyser::{Button, GeyserRemote, RemoteStatus};
use crate::scooter::{ScooterReading, ScooterState};
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
pub async fn mark_water_done(app: AppHandle, state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    let today = chrono::Local::now().date_naive();
    crate::storage::save_water_done_on(&app, today)?;
    *state.water_done_on.lock().await = Some(today);
    state
        .set_status(|status| {
            status.water.phase = super::water::WaterPhase::Done;
            status.water.note = None;
        })
        .await;
    state.wake.notify_one();
    Ok(state.status().await)
}

#[tauri::command]
pub async fn mark_geyser_done(app: AppHandle, state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    let today = chrono::Local::now().date_naive();
    crate::storage::save_geyser_done_on(&app, today)?;
    *state.geyser_done_on.lock().await = Some(today);
    state
        .set_status(|status| {
            status.loads.geyser.row.phase = super::loads::LoadPhase::Done;
            status.loads.geyser.row.note = Some("Marked done for today.".into());
        })
        .await;
    state.wake.notify_one();
    Ok(state.status().await)
}

#[tauri::command]
pub async fn skip_scooter_today(app: AppHandle, state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    let today = chrono::Local::now().date_naive();
    crate::storage::save_ev_skip_on(&app, today)?;
    *state.ev_skip_on.lock().await = Some(today);
    state
        .set_status(|status| {
            status.loads.ev.row.phase = super::loads::LoadPhase::Skipped;
            status.loads.ev.row.note = Some("Skipped for today.".into());
        })
        .await;
    state.wake.notify_one();
    Ok(state.status().await)
}

/// A power-cycled read switches the scooter on and off again, so it only
/// runs when asked for here or when the setting allows it while charging.
#[tauri::command]
pub async fn read_scooter_now(
    state: State<'_, AutomationState>,
    scooter: State<'_, ScooterState>,
    power_cycle: bool,
) -> Result<ScooterReading, String> {
    let config = state.config().await;
    let mac = Some(config.ev_ble_mac.as_str()).filter(|mac| !mac.is_empty());
    let reading = scooter.read(&config.ev_ble_name, mac, power_cycle).await;
    state.wake.notify_one();
    reading
}

async fn geyser_remote(state: &AutomationState) -> Result<GeyserRemote, String> {
    let host = state.config().await.geyser_esp32_host;
    if host.is_empty() {
        return Err("Set the ESP32's address in the geyser settings first".into());
    }
    Ok(GeyserRemote::new(&host))
}

fn button(name: &str) -> Result<Button, String> {
    Button::parse(name).ok_or_else(|| format!("unknown geyser button: {name}"))
}

#[tauri::command]
pub async fn get_geyser_remote(state: State<'_, AutomationState>) -> Result<RemoteStatus, String> {
    geyser_remote(&state).await?.status().await
}

#[tauri::command]
pub async fn learn_geyser_button(state: State<'_, AutomationState>, name: String) -> Result<RemoteStatus, String> {
    let remote = geyser_remote(&state).await?;
    remote.learn(button(&name)?).await?;
    remote.status().await
}

#[tauri::command]
pub async fn press_geyser_button(state: State<'_, AutomationState>, name: String) -> Result<(), String> {
    geyser_remote(&state).await?.send(button(&name)?).await
}

#[tauri::command]
pub async fn send_test_notification(notifier: State<'_, Notifier>) -> Result<(), String> {
    notifier.send_test(
        "Solar Hub",
        "Test notification — native banners are working.",
    );
    Ok(())
}
