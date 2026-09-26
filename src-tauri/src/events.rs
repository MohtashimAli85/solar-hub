pub const BATTERY_SNAPSHOT: &str = "battery://snapshot";
pub const BATTERY_CONNECTION: &str = "battery://connection";
pub const INVERTER_SNAPSHOT: &str = "inverter://snapshot";
pub const AUTOMATION_STATUS: &str = "automation://status";

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(error) = app.emit(event, payload) {
        tracing::warn!("failed to emit {event}: {error}");
    }
}