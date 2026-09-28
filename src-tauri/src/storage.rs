use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt as _;

use crate::automation::config::AutomationConfig;
use crate::energy::EnergyConfig;
use crate::remote::RemoteConfig;

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
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(skip)]
    pub has_gemini_api_key: bool,
    #[serde(skip)]
    pub has_groq_api_key: bool,
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

pub fn load_energy_config(app: &AppHandle) -> EnergyConfig {
    app.store(SETTINGS_STORE)
        .ok()
        .and_then(|store| store.get("energy"))
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn save_energy_config(app: &AppHandle, config: &EnergyConfig) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    store.set("energy", json!(config));
    store.save().map_err(|error| error.to_string())
}

pub fn load_remote_config(app: &AppHandle) -> Option<RemoteConfig> {
    app.store(SETTINGS_STORE)
        .ok()
        .and_then(|store| store.get("remote"))
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn save_remote_config(app: &AppHandle, config: &RemoteConfig) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    store.set("remote", json!(config));
    store.save().map_err(|error| error.to_string())
}

const KEYCHAIN_SERVICE: &str = "com.mohtashimali.solarhub";
const SOLAR_PASSWORD_ACCOUNT: &str = "solar-password";
const GEMINI_API_KEY_ACCOUNT: &str = "gemini-api-key";
const GROQ_API_KEY_ACCOUNT: &str = "groq-api-key";

fn keychain_get(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(other) => Err(other.to_string()),
    }
}

fn keychain_set(account: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| error.to_string())?;
    entry.set_password(value).map_err(|error| error.to_string())
}

fn keychain_clear(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(other.to_string()),
    }
}

/// Stores a secret, or removes it when `value` is empty.
fn keychain_save(account: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        keychain_clear(account)
    } else {
        keychain_set(account, value)
    }
}

pub fn get_solar_password() -> Result<Option<String>, String> {
    keychain_get(SOLAR_PASSWORD_ACCOUNT)
}

pub fn save_solar_password(password: &str) -> Result<(), String> {
    keychain_save(SOLAR_PASSWORD_ACCOUNT, password)
}

pub fn get_gemini_api_key() -> Result<Option<String>, String> {
    keychain_get(GEMINI_API_KEY_ACCOUNT)
}

pub fn save_gemini_api_key(key: &str) -> Result<(), String> {
    keychain_save(GEMINI_API_KEY_ACCOUNT, key)
}

pub fn get_groq_api_key() -> Result<Option<String>, String> {
    keychain_get(GROQ_API_KEY_ACCOUNT)
}

pub fn save_groq_api_key(key: &str) -> Result<(), String> {
    keychain_save(GROQ_API_KEY_ACCOUNT, key)
}
