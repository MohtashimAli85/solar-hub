use serde::Deserialize;
use tauri::{AppHandle, State};

use super::client::SolarCredentials;
use super::types::{DeviceDetails, InverterSettings, InverterSnapshot};
use super::InverterState;
use crate::battery::BatteryState;
use crate::events;
use crate::storage::{
    get_gemini_api_key, get_groq_api_key, get_solar_password, load_app_settings, save_app_settings,
    save_gemini_api_key, save_groq_api_key, save_solar_password, AppSettings,
};

#[tauri::command]
pub async fn get_inverter_snapshot(
    state: State<'_, InverterState>,
    battery: State<'_, BatteryState>,
    app: AppHandle,
) -> Result<InverterSnapshot, String> {
    let mut snapshot = state
        .solar_client()
        .read_inverter_snapshot(None)
        .await
        .map_err(|error| error.to_string())?;
    let bms = if battery.connection_status().await.connected {
        battery.latest_snapshot().await
    } else {
        None
    };
    snapshot.grid = Some(snapshot.grid_status(bms.as_ref().map(|b| b.current), bms.as_ref().map(|b| b.voltage)));
    events::emit(&app, events::INVERTER_SNAPSHOT, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn get_inverter_settings(
    state: State<'_, InverterState>,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    let device_id = match device_id {
        Some(id) if !id.trim().is_empty() => id,
        _ => state.solar_client().credentials().await.device_id,
    };
    state
        .solar_client()
        .read_inverter_settings(&device_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_output_priority(
    state: State<'_, InverterState>,
    mode: String,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_output_priority(device_id, mode)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_charger_priority(
    state: State<'_, InverterState>,
    mode: String,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_charger_priority(device_id, mode)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_ac_input_range(
    state: State<'_, InverterState>,
    enabled: bool,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_ac_input_range(device_id, enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_grid_feed_in(
    state: State<'_, InverterState>,
    enabled: bool,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_grid_feed_in(device_id, enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_smart_load(
    state: State<'_, InverterState>,
    enabled: bool,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_smart_load(device_id, enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_low_battery_cutoff_voltage(
    state: State<'_, InverterState>,
    value: f64,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_low_battery_cutoff_voltage(device_id, value)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_high_cutoff_voltage(
    state: State<'_, InverterState>,
    value: f64,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_high_cutoff_voltage(device_id, value)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_low_dc_cutoff_soc(
    state: State<'_, InverterState>,
    value: f64,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_low_dc_cutoff_soc(device_id, value)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_max_total_charge_current(
    state: State<'_, InverterState>,
    value: f64,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_max_total_charge_current(device_id, value)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_max_utility_charge_current(
    state: State<'_, InverterState>,
    value: f64,
    device_id: Option<String>,
) -> Result<InverterSettings, String> {
    state
        .solar_client()
        .write_max_utility_charge_current(device_id, value)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_device_details(
    state: State<'_, InverterState>,
    device_id: Option<String>,
) -> Result<DeviceDetails, String> {
    state
        .solar_client()
        .read_device_details(device_id)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolarSettingsInput {
    pub user_id: String,
    pub station_id: String,
    pub device_id: String,
    pub time_zone: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub gemini_api_key: Option<String>,
    #[serde(default)]
    pub groq_api_key: Option<String>,
}

/// Secrets live in the OS keychain and are never returned; the UI only learns
/// whether each one is saved.
fn with_secret_flags(mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.has_gemini_api_key = get_gemini_api_key()?.is_some();
    settings.has_groq_api_key = get_groq_api_key()?.is_some();
    Ok(settings)
}

/// `None` keeps the saved secret; an empty string removes it.
fn save_secret(value: Option<String>, save: fn(&str) -> Result<(), String>) -> Result<(), String> {
    value.map_or(Ok(()), |value| save(&value))
}

#[tauri::command]
pub async fn get_solar_settings(app: AppHandle) -> Result<AppSettings, String> {
    with_secret_flags(load_app_settings(&app))
}

#[tauri::command]
pub async fn update_solar_settings(
    app: AppHandle,
    state: State<'_, InverterState>,
    input: SolarSettingsInput,
) -> Result<AppSettings, String> {
    let settings = AppSettings {
        user_id: input.user_id.trim().to_string(),
        station_id: input.station_id.trim().to_string(),
        device_id: input.device_id.trim().to_string(),
        time_zone: input.time_zone.trim().to_string(),
        latitude: input.latitude,
        longitude: input.longitude,
        ..load_app_settings(&app)
    };
    save_app_settings(&app, &settings)?;
    save_secret(input.password, save_solar_password)?;
    save_secret(input.gemini_api_key, save_gemini_api_key)?;
    save_secret(input.groq_api_key, save_groq_api_key)?;
    let stored_password = get_solar_password()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let credentials = SolarCredentials {
        user_id: settings.user_id.clone(),
        password: stored_password,
        station_id: settings.station_id.clone(),
        device_id: settings.device_id.clone(),
        time_zone: settings.time_zone.clone(),
    };
    state.solar_client().set_credentials(credentials).await;
    with_secret_flags(settings)
}