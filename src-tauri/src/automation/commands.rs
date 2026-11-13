use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::config::AutomationConfig;
use crate::geyser::{Button, GeyserRemote, RemoteStatus};
use crate::scooter::{ScooterReading, ScooterState};
use super::history::DecisionRow;
use super::insights::AutomationInsights;
use super::loads::{EditState, LoadEdit, LoadKind, LoadPhase, LoadsAnswer};
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
pub async fn refresh_forecast(state: State<'_, AutomationState>) -> Result<(), String> {
    state.refresh_forecast.store(true, std::sync::atomic::Ordering::SeqCst);
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
            status.loads.water.phase = LoadPhase::Done;
            status.loads.water.note = Some("Filled · updating the plan…".into());
            status.loads.water_running_since = None;
            status.loads.tank_question = None;
        })
        .await;
    state.loads_touched();
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
    state.loads_touched();
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
    state.loads_touched();
    Ok(state.status().await)
}

#[tauri::command]
pub async fn answer_loads_question(state: State<'_, AutomationState>, yes: bool) -> Result<AutomationStatus, String> {
    *state.loads_answer.lock().await = Some(if yes { LoadsAnswer::Yes } else { LoadsAnswer::No });
    state.set_status(|status| status.loads.question = None).await;
    state.loads_touched();
    Ok(state.status().await)
}

/// Saves or clears one of the remembered days, keeping the app's copy in step.
async fn set_day(app: &AppHandle, cell: &tokio::sync::Mutex<Option<chrono::NaiveDate>>, key: &str, date: Option<chrono::NaiveDate>) -> Result<(), String> {
    crate::storage::save_day(app, key, date)?;
    *cell.lock().await = date;
    Ok(())
}

async fn clear_if_today(app: &AppHandle, cell: &tokio::sync::Mutex<Option<chrono::NaiveDate>>, key: &str, today: chrono::NaiveDate) -> Result<(), String> {
    if *cell.lock().await == Some(today) {
        set_day(app, cell, key, None).await?;
    }
    Ok(())
}

/// The family correcting what the app believes about a load.
#[tauri::command]
pub async fn edit_load(app: AppHandle, state: State<'_, AutomationState>, edit: LoadEdit) -> Result<AutomationStatus, String> {
    let today = chrono::Local::now().date_naive();
    match (edit.load, edit.state) {
        (LoadKind::Water, EditState::Done) => set_day(&app, &state.water_done_on, "water_done_on", Some(today)).await?,
        (LoadKind::Water, _) => clear_if_today(&app, &state.water_done_on, "water_done_on", today).await?,
        (LoadKind::Geyser, EditState::Done) => set_day(&app, &state.geyser_done_on, "geyser_done_on", Some(today)).await?,
        (LoadKind::Geyser, _) => clear_if_today(&app, &state.geyser_done_on, "geyser_done_on", today).await?,
        (LoadKind::Hotplate, EditState::Off) => clear_if_today(&app, &state.hotplate_on, "hotplate_on", today).await?,
        (LoadKind::Hotplate, _) => {
            set_day(&app, &state.hotplate_on, "hotplate_on", Some(today)).await?;
            clear_if_today(&app, &state.hotplate_skip_on, "hotplate_skip_on", today).await?;
        }
        (LoadKind::Ev, EditState::On) => clear_if_today(&app, &state.ev_skip_on, "ev_skip_on", today).await?,
        (LoadKind::Ev, _) => {}
    }
    let (phase, note) = match edit.state {
        EditState::On => (LoadPhase::Running, "On · updating the plan…"),
        EditState::Off => (LoadPhase::Waiting, "Off · updating the plan…"),
        EditState::Done => (LoadPhase::Done, "Done · updating the plan…"),
    };
    let kind = edit.load;
    state
        .set_status(|status| {
            let loads = &mut status.loads;
            let row = match kind {
                LoadKind::Water => &mut loads.water,
                LoadKind::Geyser => &mut loads.geyser.row,
                LoadKind::Hotplate => &mut loads.hotplate.row,
                LoadKind::Ev => &mut loads.ev.row,
            };
            row.phase = phase;
            row.note = Some(note.into());
            if kind == LoadKind::Water {
                loads.tank_question = None;
                if phase == LoadPhase::Done {
                    status.water.phase = super::water::WaterPhase::Done;
                }
            }
        })
        .await;
    state.loads_edits.lock().await.push(edit);
    state.loads_touched();
    Ok(state.status().await)
}

#[tauri::command]
pub async fn hotplate_done(state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    *state.loads_answer.lock().await = Some(LoadsAnswer::HotplateDone);
    state
        .set_status(|status| {
            status.loads.hotplate.row.phase = LoadPhase::Done;
            status.loads.hotplate.row.note = Some("Done cooking · updating the plan…".into());
        })
        .await;
    state.loads_touched();
    Ok(state.status().await)
}

#[tauri::command]
pub async fn skip_hotplate_today(app: AppHandle, state: State<'_, AutomationState>) -> Result<AutomationStatus, String> {
    let today = chrono::Local::now().date_naive();
    crate::storage::save_hotplate_skip_on(&app, today)?;
    *state.hotplate_skip_on.lock().await = Some(today);
    state
        .set_status(|status| {
            status.loads.hotplate.row.phase = super::loads::LoadPhase::Skipped;
            status.loads.hotplate.row.note = Some("Not cooking on the hotplate today.".into());
        })
        .await;
    state.loads_touched();
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
    state.loads_touched();
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
