use tauri::{AppHandle, State};

use super::manager::{connect_device, disconnect_device, reconnect_saved, scan_devices};
use super::types::BatterySnapshot;
use super::BatteryState;
use crate::storage::{SavedBleDevice, load_saved_ble_device, save_saved_ble_device};

#[tauri::command]
pub async fn scan_bms_devices(
    state: State<'_, BatteryState>,
) -> Result<Vec<crate::battery::types::DiscoveredDevice>, String> {
    let _op_lock = state.inner.op_lock.lock().await;
    scan_devices(&state).await
}

#[tauri::command]
pub async fn get_saved_bms_device(app: AppHandle) -> Result<Option<SavedBleDevice>, String> {
    Ok(load_saved_ble_device(&app))
}

#[tauri::command]
pub async fn set_saved_bms_device(
    app: AppHandle,
    device_id: String,
    device_name: String,
) -> Result<(), String> {
    save_saved_ble_device(
        &app,
        &SavedBleDevice {
            id: device_id,
            name: device_name,
        },
    )
}

#[tauri::command]
pub async fn get_battery_state(
    state: State<'_, BatteryState>,
) -> Result<Option<BatterySnapshot>, String> {
    Ok(state.latest_snapshot().await)
}

#[tauri::command]
pub async fn get_battery_connection(
    state: State<'_, BatteryState>,
) -> Result<crate::battery::types::ConnectionStatus, String> {
    Ok(state.connection_status().await)
}

#[tauri::command]
pub async fn connect_bms_device(
    app: AppHandle,
    state: State<'_, BatteryState>,
    device_id: String,
) -> Result<(), String> {
    let _op_lock = state.inner.op_lock.lock().await;
    connect_device(&state, &app, &device_id).await
}

#[tauri::command]
pub async fn disconnect_bms_device(
    app: AppHandle,
    state: State<'_, BatteryState>,
) -> Result<(), String> {
    disconnect_device(&state, &app).await
}

#[tauri::command]
pub async fn reconnect_saved_bms(
    app: AppHandle,
    state: State<'_, BatteryState>,
) -> Result<(), String> {
    let _op_lock = state.inner.op_lock.lock().await;
    reconnect_saved(&state, &app).await
}