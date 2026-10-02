pub const BATTERY_SNAPSHOT: &str = "battery://snapshot";
pub const BATTERY_CONNECTION: &str = "battery://connection";
pub const INVERTER_SNAPSHOT: &str = "inverter://snapshot";
pub const AUTOMATION_STATUS: &str = "automation://status";
pub const ENERGY_UPDATED: &str = "energy://updated";
pub const SYSTEM_RESUMED: &str = "system://resumed";

use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

pub type RemoteEvent = (String, String);

fn remote_channel() -> &'static broadcast::Sender<RemoteEvent> {
    static CHANNEL: OnceLock<broadcast::Sender<RemoteEvent>> = OnceLock::new();
    CHANNEL.get_or_init(|| broadcast::channel(64).0)
}

pub fn subscribe_remote() -> broadcast::Receiver<RemoteEvent> {
    remote_channel().subscribe()
}

pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    let channel = remote_channel();
    if channel.receiver_count() > 0 {
        if let Ok(json) = serde_json::to_string(&payload) {
            let _ = channel.send((event.to_string(), json));
        }
    }
    if let Err(error) = app.emit(event, payload) {
        tracing::warn!("failed to emit {event}: {error}");
    }
}
