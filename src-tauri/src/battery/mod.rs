pub mod commands;
pub mod jbd_protocol;
pub mod manager;
pub mod types;

use std::sync::Arc;
use std::time::Instant;

use btleplug::platform::{Adapter, Peripheral};
use tokio::sync::{Mutex, Notify};

use self::jbd_protocol::BasicStatus;
use self::types::BatterySnapshot;

#[derive(Debug, Clone, Default)]
pub struct BatteryMeta {
    pub device_id: String,
    pub device_name: String,
}

pub struct BatteryInner {
    pub central: Mutex<Option<Adapter>>,
    pub peripherals: Mutex<Vec<Peripheral>>,
    pub devices: Mutex<Vec<types::DiscoveredDevice>>,
    pub connected: Mutex<Option<Peripheral>>,
    pub meta: Mutex<BatteryMeta>,
    pub latest_basic: Mutex<Option<BasicStatus>>,
    pub latest_cells: Mutex<Vec<f64>>,
    pub latest: Mutex<Option<BatterySnapshot>>,
    pub reconnect_generation: Mutex<u64>,
    pub reconnecting: Mutex<bool>,
    /// Timestamp of the last successfully-parsed BMS frame (or of connect
    /// time, if none yet) — a link can report "connected" while the BMS has
    /// stopped actually talking; this is the app-level liveness signal for
    /// that case, independent of what the OS/BLE stack reports.
    pub last_frame_at: Mutex<Option<Instant>>,
    /// Serializes BLE scans and connects so the manual commands and the
    /// background reconnect loop cannot run two operations at once.
    pub op_lock: Mutex<()>,
    /// Fired whenever the cached adapter is dropped, so the disconnect
    /// watcher knows to re-subscribe to the new adapter's events.
    pub central_changed: Notify,
    pub session_tasks: Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
    pub reconnect_kick: Notify,
}

impl BatteryInner {
    pub fn new() -> Self {
        Self {
            central: Mutex::new(None),
            peripherals: Mutex::new(Vec::new()),
            devices: Mutex::new(Vec::new()),
            connected: Mutex::new(None),
            meta: Mutex::new(BatteryMeta::default()),
            latest_basic: Mutex::new(None),
            latest_cells: Mutex::new(Vec::new()),
            latest: Mutex::new(None),
            reconnect_generation: Mutex::new(0),
            reconnecting: Mutex::new(false),
            last_frame_at: Mutex::new(None),
            op_lock: Mutex::new(()),
            central_changed: Notify::new(),
            session_tasks: Mutex::new(Vec::new()),
            reconnect_kick: Notify::new(),
        }
    }
}

#[derive(Clone)]
pub struct BatteryState {
    pub inner: Arc<BatteryInner>,
}

impl BatteryState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BatteryInner::new()),
        }
    }

    pub async fn latest_snapshot(&self) -> Option<BatterySnapshot> {
        self.inner.latest.lock().await.clone()
    }

    pub async fn connection_status(&self) -> types::ConnectionStatus {
        let connected_handle = self.inner.connected.lock().await.clone();
        let connected = match connected_handle.as_ref() {
            Some(peripheral) => manager::is_connected(peripheral).await,
            None => false,
        };
        let meta = self.inner.meta.lock().await;
        types::ConnectionStatus {
            connected,
            device_id: if connected { Some(meta.device_id.clone()) } else { None },
            device_name: if connected { Some(meta.device_name.clone()) } else { None },
            reconnecting: *self.inner.reconnecting.lock().await,
        }
    }
}