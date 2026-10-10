use tauri::{AppHandle, State};

use chrono::NaiveDateTime;

use super::units::{BilledUnits, MeterAssignment, MeterReading, MeterSwitch};
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

fn parse_moment(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| "Pick a date and a time.".to_string())
}

#[tauri::command]
pub async fn assign_meter_range(
    app: AppHandle,
    state: State<'_, EnergyState>,
    from: String,
    to: String,
    meter: u8,
) -> Result<EnergySummary, String> {
    if !matches!(meter, 1 | 2) {
        return Err("Pick meter 1 or meter 2.".into());
    }
    let (from, to) = (parse_moment(&from)?, parse_moment(&to)?);
    if from >= to {
        return Err("The start has to be before the end.".into());
    }
    if to > state.now().await {
        return Err("The end hasn't come yet.".into());
    }
    state.records.add_assignment(MeterAssignment { from, to, meter })?;
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}

#[tauri::command]
pub async fn remove_meter_assignment(
    app: AppHandle,
    state: State<'_, EnergyState>,
    from: String,
) -> Result<EnergySummary, String> {
    state.records.remove_assignment(parse_moment(&from)?)?;
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}

/// Readings this close to the bill start are the bill's opening reading, so a
/// new one replaces them.
const OPENING_WINDOW_DAYS: i64 = 4;

/// Saves what both meters showed around the reading that opened this bill
/// (at `base_at`, the bill start when not given) and at `at` (now when not
/// given).
#[tauri::command]
pub async fn save_meter_readings(
    app: AppHandle,
    state: State<'_, EnergyState>,
    base_at: Option<String>,
    base1: f64,
    base2: f64,
    at: Option<String>,
    meter1: f64,
    meter2: f64,
) -> Result<EnergySummary, String> {
    if [base1, base2, meter1, meter2].iter().any(|value| !value.is_finite() || *value < 0.0) {
        return Err("Enter each reading as a number.".into());
    }
    if meter1 < base1 || meter2 < base2 {
        return Err("A meter can't read less now than on the last bill.".into());
    }
    let now = state.now().await;
    let reading = state.config().await.reading();
    let (start, previous_start) = (reading.period_start(now), reading.previous_period_start(now));
    let base_at = match base_at.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => parse_moment(text)?,
        None => start,
    };
    let at = match at.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => parse_moment(text)?,
        None => now,
    };
    if base_at < previous_start || base_at >= now {
        return Err("Pick when the photo was taken: not in the future, and not before the previous bill.".into());
    }
    if at <= base_at || at > now {
        return Err("The latest reading has to be after the photo, and not in the future.".into());
    }
    for old in state.records.meter_readings() {
        if old.at != base_at && (old.at - start).num_days().abs() <= OPENING_WINDOW_DAYS {
            state.records.remove_reading(old.at)?;
        }
    }
    state.records.add_reading(MeterReading { at: base_at, meter1: base1, meter2: base2 })?;
    state.records.add_reading(MeterReading { at, meter1, meter2 })?;
    state.wake.notify_one();
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}

/// Saves the units the last bill charged each meter, to compare with the count.
#[tauri::command]
pub async fn save_last_bill_units(
    app: AppHandle,
    state: State<'_, EnergyState>,
    meter1: f64,
    meter2: f64,
) -> Result<EnergySummary, String> {
    if [meter1, meter2].iter().any(|value| !value.is_finite() || *value < 0.0) {
        return Err("Enter each meter's units as a number.".into());
    }
    let now = state.now().await;
    let start = state.config().await.reading().previous_period_start(now);
    state.records.set_billed_units(BilledUnits { start, meter1, meter2 })?;
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}

#[tauri::command]
pub async fn remove_meter_reading(app: AppHandle, state: State<'_, EnergyState>, at: String) -> Result<EnergySummary, String> {
    state.records.remove_reading(parse_moment(&at)?)?;
    let summary = state.rebuild().await;
    events::emit(&app, events::ENERGY_UPDATED, summary.clone());
    Ok(summary)
}
