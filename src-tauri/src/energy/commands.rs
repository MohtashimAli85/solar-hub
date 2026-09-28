use tauri::{AppHandle, State};

use super::units::MeterSwitch;
use super::{parse_reading_time, switch_time, EnergyConfig, EnergyState, EnergySummary, MAX_STANDBY_W};
use crate::events;

#[tauri::command]
pub async fn get_energy_summary(state: State<'_, EnergyState>) -> Result<EnergySummary, String> {
    match state.summary().await {
        Some(summary) => Ok(summary),
        None => Ok(state.rebuild().await),
    }
}

#[tauri::command]
pub async fn set_bill_reading(
    app: AppHandle,
    state: State<'_, EnergyState>,
    day: u32,
    time: String,
) -> Result<EnergySummary, String> {
    let time = parse_reading_time(&time).ok_or_else(|| "Use a time like 11:57.".to_string())?;
    let config = EnergyConfig {
        bill_reading_day: day.clamp(1, 28),
        bill_reading_time: time.format("%H:%M").to_string(),
        ..state.config().await
    };
    let summary = save(&app, &state, config).await?;
    state.wake.notify_one();
    Ok(summary)
}

#[tauri::command]
pub async fn set_standby_watts(
    app: AppHandle,
    state: State<'_, EnergyState>,
    watts: f64,
) -> Result<EnergySummary, String> {
    if !watts.is_finite() {
        return Err("Enter the watts as a number.".into());
    }
    let config = EnergyConfig { standby_w: watts.clamp(0.0, MAX_STANDBY_W), ..state.config().await };
    save(&app, &state, config).await
}

async fn save(app: &AppHandle, state: &EnergyState, config: EnergyConfig) -> Result<EnergySummary, String> {
    crate::storage::save_energy_config(app, &config)?;
    state.set_config(config).await;
    let summary = state.rebuild().await;
    events::emit(app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}

#[tauri::command]
pub async fn set_active_meter(
    app: AppHandle,
    state: State<'_, EnergyState>,
    meter: u8,
    at: Option<String>,
) -> Result<EnergySummary, String> {
    if !matches!(meter, 1 | 2) {
        return Err("Pick meter 1 or meter 2.".into());
    }
    let last = state.records.meter_switches().last().map(|switch| switch.at);
    let when = switch_time(state.now().await, last, at.as_deref())?;
    state
        .records
        .append_switch(MeterSwitch { at: when, meter })
        .map_err(|error| format!("Couldn't save the meter change: {error}"))?;
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}
