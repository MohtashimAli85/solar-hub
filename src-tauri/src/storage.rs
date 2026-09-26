use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt as _;

use crate::automation::config::AutomationConfig;

const SETTINGS_STORE: &str = "settings.json";
const AUTOMATION_STORE: &str = "automation.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub user_id: String,
    pub station_id: String,
    pub device_id: String,
    pub time_zone: String,
    pub saved_ble_device_id: String,
    pub saved_ble_device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedBleDevice {
    pub id: String,
    pub name: String,
}

pub fn load_app_settings(app: &AppHandle) -> AppSettings {
    app.store(SETTINGS_STORE)
        .ok()
        .and_then(|store| store.get("settings"))
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn save_app_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    store.set("settings", json!(settings));
    store.save().map_err(|error| error.to_string())
}

pub fn load_saved_ble_device(app: &AppHandle) -> Option<SavedBleDevice> {
    app.store(SETTINGS_STORE)
        .ok()
        .and_then(|store| store.get("saved_ble_device"))
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn save_saved_ble_device(app: &AppHandle, device: &SavedBleDevice) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    store.set("saved_ble_device", json!(device));
    store.save().map_err(|error| error.to_string())
}

pub fn load_automation_config(app: &AppHandle) -> AutomationConfig {
    app.store(AUTOMATION_STORE)
        .ok()
        .and_then(|store| store.get("config"))
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn save_automation_config(app: &AppHandle, config: &AutomationConfig) -> Result<(), String> {
    let store = app
        .store(AUTOMATION_STORE)
        .map_err(|error| error.to_string())?;
    store.set("config", json!(config));
    store.save().map_err(|error| error.to_string())
}

const KEYCHAIN_SERVICE: &str = "com.mohtashimali.solarhub";
const KEYCHAIN_ACCOUNT: &str = "solar-password";

pub fn get_solar_password() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(other) => Err(other.to_string()),
    }
}

pub fn set_solar_password(password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|error| error.to_string())?;
    entry
        .set_password(password)
        .map_err(|error| error.to_string())
}

pub fn clear_solar_password() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(other.to_string()),
    }
}