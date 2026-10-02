use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central as _, CentralEvent, CentralState, CharPropFlags, Manager as _, Peripheral as _,
    RetrievePeripheralsOptions, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures_util::TryFutureExt as _;
use futures_util::StreamExt as _;
use tauri::AppHandle;

use super::jbd_protocol::{
    CELL_VOLTAGE_COMMAND, COMMAND_BASIC_STATUS, COMMAND_CELL_VOLTAGE,
    NOTIFY_CHARACTERISTIC_UUID, STATUS_COMMAND, WRITE_CHARACTERISTIC_ALT_UUID,
    WRITE_CHARACTERISTIC_UUID, drain_complete_frames, parse_basic_status,
    parse_cell_voltages,
};
use super::types::{BatterySnapshot, ConnectionStatus, DiscoveredDevice};
use super::{BatteryMeta, BatteryState};
use crate::events;
use crate::notifications::{Notifier, Reading};
use crate::storage::{SavedBleDevice, load_saved_ble_device, save_saved_ble_device};

const SCAN_DURATION: Duration = Duration::from_millis(6000);
const RECONNECT_BACKOFF: [u64; 5] = [3, 5, 10, 15, 30];
/// The poll loop writes a status+cell-voltage request roughly every ~2.1s;
/// if no frame has come back in this long, the BMS side of the link is dead
/// even though the OS may still report the connection as up (e.g. its BLE
/// radio keeps ACKing writes while its main MCU has stopped responding).
const STALE_DATA_TIMEOUT: Duration = Duration::from_secs(18);

async fn with_timeout<T>(
    duration: Duration,
    label: &str,
    fut: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(duration, fut).await {
        Ok(result) => result,
        Err(_) => Err(format!("{label} timed out after {duration:?}")),
    }
}

pub(super) async fn is_connected(peripheral: &btleplug::platform::Peripheral) -> bool {
    with_timeout(
        Duration::from_secs(5),
        "check connection state",
        peripheral.is_connected().map_err(|error| error.to_string()),
    )
    .await
    .unwrap_or(false)
}

async fn get_central(state: &BatteryState) -> Result<Adapter, String> {
    let mut cached = state.inner.central.lock().await;
    if let Some(central) = cached.as_ref() {
        return Ok(central.clone());
    }
    let manager = with_timeout(
        Duration::from_secs(8),
        "create Bluetooth manager",
        Manager::new().map_err(|error| error.to_string()),
    )
    .await?;
    let central = with_timeout(
        Duration::from_secs(8),
        "list Bluetooth adapters",
        async {
            let adapters = manager.adapters().await.map_err(|error| error.to_string())?;
            adapters
                .into_iter()
                .next()
                .ok_or_else(|| "No Bluetooth adapter was found on this Mac".to_string())
        },
    )
    .await?;
    *cached = Some(central.clone());
    Ok(central)
}

/// Drops the cached adapter and all scan handles so the next scan starts from
/// a fresh CoreBluetooth manager. Cached `Peripheral` handles die with their
/// adapter, so keeping them around is what left reconnect stuck in a loop of
/// "Peripheral no longer available" failures. Notifies the disconnect watcher
/// so it re-subscribes to the new adapter's events.
async fn reset_central(state: &BatteryState) {
    if state.inner.connected.lock().await.is_some() {
        return;
    }
    *state.inner.central.lock().await = None;
    state.inner.peripherals.lock().await.clear();
    state.inner.devices.lock().await.clear();
    state.inner.central_changed.notify_one();
}

pub async fn scan_devices(state: &BatteryState) -> Result<Vec<DiscoveredDevice>, String> {
    scan_devices_inner(state, None).await
}

async fn scan_devices_inner(
    state: &BatteryState,
    target: Option<&str>,
) -> Result<Vec<DiscoveredDevice>, String> {
    let central = get_central(state).await?;

    let _ = with_timeout(
        Duration::from_secs(8),
        "stop Bluetooth scan",
        central.stop_scan().map_err(|error| error.to_string()),
    )
    .await;
    let mut central_events = with_timeout(
        Duration::from_secs(8),
        "subscribe to Bluetooth scan events",
        central.events().map_err(|error| error.to_string()),
    )
    .await?;
    with_timeout(
        Duration::from_secs(8),
        "start Bluetooth scan",
        central
            .start_scan(ScanFilter::default())
            .map_err(|error| format!("unable to start Bluetooth scan: {error}")),
    )
    .await?;
    eprintln!("[battery] scan started on macOS adapter");

    let deadline = tokio::time::Instant::now() + SCAN_DURATION;
    let mut discovered_events = 0usize;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, central_events.next()).await {
            Ok(Some(event)) => match event {
                btleplug::api::CentralEvent::DeviceDiscovered(id) => {
                    discovered_events += 1;
                    eprintln!("[battery] DeviceDiscovered: {id:?}");
                    if target.is_some_and(|target| id.to_string() == target) {
                        eprintln!("[battery] target device discovered early, stopping scan");
                        break;
                    }
                }
                btleplug::api::CentralEvent::DeviceUpdated(id) => {
                    eprintln!("[battery] DeviceUpdated: {id:?}");
                }
                other => eprintln!("[battery] other central event: {other:?}"),
            },
            Ok(None) => break,
            Err(_) => break,
        }
    }
    let _ = with_timeout(
        Duration::from_secs(8),
        "stop Bluetooth scan",
        central.stop_scan().map_err(|error| error.to_string()),
    )
    .await;
    eprintln!("[battery] scan finished, discovery events seen: {discovered_events}");

    // Exclude ourself (the Mac's own adapter is not discoverable, but some
    // platforms report it) and snapshot everything found.
    let peripherals = with_timeout(
        Duration::from_secs(8),
        "list Bluetooth peripherals",
        central.peripherals().map_err(|error| error.to_string()),
    )
    .await?;
    let mut devices: Vec<DiscoveredDevice> = Vec::new();
    for peripheral in &peripherals {
        let properties = peripheral
            .properties()
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let id = peripheral.id().to_string();
        if devices.iter().any(|device| device.id == id) {
            continue;
        }
        eprintln!(
            "[battery] peripheral id={id} name={name:?} addr={addr:?} rssi={rssi:?}",
            name = properties.local_name,
            addr = properties.address.to_string(),
            rssi = properties.rssi,
        );
        let name = properties.local_name.clone().filter(|name| !name.is_empty());
        let name = name.unwrap_or_else(|| {
            let is_jbd = properties.service_data.keys().any(|uuid| {
                let uuid = uuid.to_string().to_lowercase();
                uuid.ends_with("ff01") || uuid.ends_with("ff02")
            });
            if is_jbd { "JBD BMS".to_string() } else { "Unnamed BMS".to_string() }
        });
        devices.push(DiscoveredDevice {
            id,
            name,
            rssi: properties.rssi,
        });
    }

    *state.inner.peripherals.lock().await = peripherals;
    *state.inner.devices.lock().await = devices.clone();
    Ok(devices)
}

async fn teardown_session(state: &BatteryState, peripheral: Option<Peripheral>) {
    let tasks = std::mem::take(&mut *state.inner.session_tasks.lock().await);
    for task in tasks {
        task.abort();
    }
    if let Some(peripheral) = peripheral {
        let _ = with_timeout(
            Duration::from_secs(5),
            "disconnect BMS",
            peripheral.disconnect().map_err(|error| error.to_string()),
        )
        .await;
    }
}

pub async fn mark_disconnected(
    state: &BatteryState,
    app: &AppHandle,
    peripheral_id: PeripheralId,
) -> Option<Peripheral> {
    let mut connected = state.inner.connected.lock().await;
    let matches = matches!(
        connected.as_ref(),
        Some(peripheral) if peripheral.id() == peripheral_id
    );
    if !matches {
        return None;
    }
    let peripheral = connected.take();
    drop(connected);
    let (device_id, device_name) = {
        let meta = state.inner.meta.lock().await;
        (meta.device_id.clone(), meta.device_name.clone())
    };
    let reconnecting = *state.inner.reconnecting.lock().await;
    events::emit(
        app,
        events::BATTERY_CONNECTION,
        ConnectionStatus {
            connected: false,
            device_id: if device_id.is_empty() {
                None
            } else {
                Some(device_id)
            },
            device_name: if device_name.is_empty() {
                None
            } else {
                Some(device_name)
            },
            reconnecting,
        },
    );
    peripheral
}

pub fn handle_link_lost(
    state: BatteryState,
    app: AppHandle,
    peripheral_id: PeripheralId,
    reason: &str,
) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let Some(peripheral) = mark_disconnected(&state, &app, peripheral_id).await else {
            return;
        };
        tracing::warn!("BMS link lost ({reason}), tearing down session");
        teardown_session(&state, Some(peripheral)).await;
        spawn_reconnect_loop(state, app);
    });
}

async fn bluetooth_powered_off(central: &Adapter) -> bool {
    matches!(
        with_timeout(
            Duration::from_secs(5),
            "read Bluetooth state",
            central.adapter_state().map_err(|error| error.to_string()),
        )
        .await,
        Ok(CentralState::PoweredOff)
    )
}

/// The battery % comes over Bluetooth, so when it is switched off the app
/// turns it back on, and only asks the user when that doesn't work.
fn handle_bluetooth_off(state: BatteryState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if !load_saved_ble_device(&app).is_some_and(|device| !device.id.is_empty()) {
            return;
        }
        if state.inner.bluetooth_off_handled.swap(true, Ordering::SeqCst) {
            return;
        }
        tracing::warn!("Bluetooth is off, trying to turn it on");
        if super::power::request_on() {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if super::power::is_on() == Some(true) {
                    tracing::info!("Bluetooth turned back on");
                    state.inner.reconnect_kick.notify_one();
                    return;
                }
            }
        }
        tracing::warn!("could not turn Bluetooth on");
        if let Some(notifier) = tauri::Manager::try_state::<Notifier>(&app) {
            notifier.send("Bluetooth is off", "Turn Bluetooth on so Solar Hub can read the battery.");
        }
    });
}

pub async fn on_resume(state: BatteryState, app: AppHandle) {
    if let Ok(central) = get_central(&state).await {
        if bluetooth_powered_off(&central).await {
            handle_bluetooth_off(state, app);
            return;
        }
    }
    if !state.connection_status().await.connected {
        spawn_reconnect_loop(state.clone(), app);
        state.inner.reconnect_kick.notify_one();
    }
}

pub fn spawn_disconnect_watcher(state: BatteryState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tracing::info!("disconnect watcher started");
        loop {
            let central = match get_central(&state).await {
                Ok(central) => central,
                Err(error) => {
                    tracing::warn!("disconnect watcher: no Bluetooth adapter: {error}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            let mut events = match central.events().await {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!("disconnect watcher: event subscription failed: {error}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            tracing::info!("disconnect watcher listening on adapter");
            if bluetooth_powered_off(&central).await {
                handle_bluetooth_off(state.clone(), app.clone());
            }
            loop {
                tokio::select! {
                    event = events.next() => {
                        match event {
                            Some(CentralEvent::DeviceDisconnected(id)) => {
                                tracing::warn!("CentralEvent::DeviceDisconnected: {id:?}");
                                handle_link_lost(
                                    state.clone(),
                                    app.clone(),
                                    id,
                                    "adapter reported disconnect",
                                );
                            }
                            Some(CentralEvent::StateUpdate(CentralState::PoweredOn)) => {
                                state.inner.bluetooth_off_handled.store(false, Ordering::SeqCst);
                                state.inner.reconnect_kick.notify_one();
                            }
                            Some(CentralEvent::StateUpdate(CentralState::PoweredOff)) => {
                                handle_bluetooth_off(state.clone(), app.clone());
                            }
                            Some(_) => {}
                            None => {
                                tracing::warn!("disconnect watcher event stream ended unexpectedly, re-subscribing");
                                break;
                            }
                        }
                    }
                    _ = state.inner.central_changed.notified() => {
                        tracing::info!("disconnect watcher: adapter replaced, re-subscribing");
                        break;
                    }
                }
            }
        }
    });
}

pub fn spawn_reconnect_loop(state: BatteryState, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match load_saved_ble_device(&app) {
            Some(device) if !device.id.is_empty() => {}
            _ => return,
        }
        {
            let mut reconnecting = state.inner.reconnecting.lock().await;
            if *reconnecting {
                return;
            }
            *reconnecting = true;
        }
        let generation = *state.inner.reconnect_generation.lock().await;
        let (device_id, device_name) = {
            let meta = state.inner.meta.lock().await;
            (meta.device_id.clone(), meta.device_name.clone())
        };
        events::emit(
            &app,
            events::BATTERY_CONNECTION,
            ConnectionStatus {
                connected: false,
                device_id: if device_id.is_empty() {
                    None
                } else {
                    Some(device_id)
                },
                device_name: if device_name.is_empty() {
                    None
                } else {
                    Some(device_name)
                },
                reconnecting: true,
            },
        );
        tracing::info!("reconnect loop started (generation {generation})");
        let mut attempt = 0usize;
        loop {
            if *state.inner.reconnect_generation.lock().await != generation {
                tracing::info!("reconnect loop superseded by manual action");
                break;
            }
            let attempt_result = {
                let _op_lock = state.inner.op_lock.lock().await;
                reconnect_saved(&state, &app).await
            };
            match attempt_result {
                Ok(()) => {
                    tracing::info!("reconnect loop connected to saved BMS");
                    break;
                }
                Err(error) => {
                    tracing::warn!("reconnect attempt {attempt} failed: {error}");
                    attempt += 1;
                    if attempt.is_multiple_of(3) {
                        reset_central(&state).await;
                    }
                }
            }
            let delay = RECONNECT_BACKOFF[attempt.min(RECONNECT_BACKOFF.len() - 1)];
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
                _ = state.inner.reconnect_kick.notified() => {
                    tracing::info!("reconnect loop woken early by adapter power-on");
                }
            }
        }
    });
}

pub async fn connect_device(
    state: &BatteryState,
    app: &AppHandle,
    id: &str,
) -> Result<(), String> {
    teardown_session(state, None).await;
    {
        let connected = state.inner.connected.lock().await;
        if let Some(peripheral) = connected.as_ref() {
            if peripheral.id().to_string() == id && is_connected(peripheral).await {
                return Ok(());
            }
        }
    }

    let peripherals = state.inner.peripherals.lock().await.clone();
    let peripheral = peripherals
        .iter()
        .find(|peripheral| peripheral.id().to_string() == id)
        .ok_or_else(|| {
            "Device is not in the scan results yet — scan again before connecting".to_string()
        })?
        .clone();

    let setup = async {
        with_timeout(
            Duration::from_secs(15),
            "connect to BMS",
            peripheral
                .connect()
                .map_err(|error| format!("connection failed: {error}")),
        )
        .await?;
        with_timeout(
            Duration::from_secs(15),
            "discover BMS services",
            peripheral
                .discover_services()
                .map_err(|error| format!("service discovery failed: {error}")),
        )
        .await?;

        let characteristics = peripheral.characteristics();
        eprintln!("[battery] discovered characteristics:");
        for characteristic in &characteristics {
            eprintln!(
                "[battery]   {uuid} props={props:?}",
                uuid = characteristic.uuid,
                props = characteristic.properties,
            );
        }
        let notify_characteristic = characteristics
            .iter()
            .find(|characteristic| characteristic.uuid == NOTIFY_CHARACTERISTIC_UUID)
            .ok_or_else(|| "BMS does not expose the 0xFF01 notify characteristic".to_string())?
            .clone();
        let write_characteristic = characteristics
            .iter()
            .find(|characteristic| {
                characteristic.uuid == WRITE_CHARACTERISTIC_UUID
                    && (characteristic
                        .properties
                        .contains(btleplug::api::CharPropFlags::WRITE)
                        || characteristic
                            .properties
                            .contains(btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE))
            })
            .or_else(|| {
                characteristics.iter().find(|characteristic| {
                    characteristic.uuid == WRITE_CHARACTERISTIC_ALT_UUID
                        && (characteristic
                            .properties
                            .contains(btleplug::api::CharPropFlags::WRITE)
                            || characteristic
                                .properties
                                .contains(btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE))
                })
            })
            .ok_or_else(|| "BMS does not expose a writable JBD write characteristic".to_string())?
            .clone();
        eprintln!("[battery] using write characteristic {write_characteristic:?}");

        let properties = peripheral.properties().await.ok().flatten().unwrap_or_default();
        let name = properties.local_name.unwrap_or_default();

        with_timeout(
            Duration::from_secs(10),
            "subscribe to BMS notifications",
            peripheral
                .subscribe(&notify_characteristic)
                .map_err(|error| format!("failed to subscribe to notifications: {error}")),
        )
        .await?;

        Ok::<_, String>((notify_characteristic, write_characteristic, name))
    }
    .await;

    let (notify_characteristic, write_characteristic, name) = match setup {
        Ok(setup) => setup,
        Err(error) => {
            // Cancel a pending CoreBluetooth connect so it can't grab the BMS
            // on an abandoned manager once this attempt is dropped.
            let _ = with_timeout(
                Duration::from_secs(5),
                "cancel failed BMS connect",
                peripheral.disconnect().map_err(|error| error.to_string()),
            )
            .await;
            return Err(error);
        }
    };

    {
        let mut meta = state.inner.meta.lock().await;
        *meta = BatteryMeta {
            device_id: id.to_string(),
            device_name: name.clone(),
        };
        let mut connected = state.inner.connected.lock().await;
        *connected = Some(peripheral.clone());
    }
    *state.inner.reconnect_generation.lock().await += 1;
    *state.inner.reconnecting.lock().await = false;
    *state.inner.last_frame_at.lock().await = Some(Instant::now());

    let _ = save_saved_ble_device(
        app,
        &SavedBleDevice {
            id: id.to_string(),
            name: name.clone(),
        },
    );

    let notification_task = spawn_notification_handler(
        state.clone(),
        app.clone(),
        peripheral.clone(),
        notify_characteristic,
    );
    let polling_task = spawn_polling_task(
        state.clone(),
        app.clone(),
        peripheral.clone(),
        write_characteristic,
    );
    *state.inner.session_tasks.lock().await = vec![notification_task, polling_task];

    tracing::info!("connected to BMS {id} ({name})");
    events::emit(
        app,
        events::BATTERY_CONNECTION,
        ConnectionStatus {
            connected: true,
            device_id: Some(id.to_string()),
            device_name: Some(name),
            reconnecting: false,
        },
    );
    Ok(())
}

pub async fn disconnect_device(state: &BatteryState, app: &AppHandle) -> Result<(), String> {
    tracing::info!("manual disconnect requested");
    *state.inner.reconnect_generation.lock().await += 1;
    *state.inner.reconnecting.lock().await = false;
    teardown_session(state, None).await;
    let peripheral = state.inner.connected.lock().await.clone();
    *state.inner.connected.lock().await = None;
    let device_id = state.inner.meta.lock().await.device_id.clone();
    events::emit(
        app,
        events::BATTERY_CONNECTION,
        ConnectionStatus {
            connected: false,
            device_id: Some(device_id),
            device_name: None,
            reconnecting: false,
        },
    );
    if let Some(peripheral) = peripheral {
        let _ = with_timeout(
            Duration::from_secs(5),
            "manual disconnect",
            peripheral.disconnect().map_err(|error| error.to_string()),
        )
        .await;
    }
    Ok(())
}

pub async fn reconnect_saved(state: &BatteryState, app: &AppHandle) -> Result<(), String> {
    let saved = match load_saved_ble_device(app) {
        Some(device) if !device.id.is_empty() => device,
        _ => {
            return Err(
                "No saved BMS device found. Scan and connect once before using saved reconnect."
                    .to_string(),
            );
        }
    };

    {
        let connected = state.inner.connected.lock().await;
        if let Some(peripheral) = connected.as_ref() {
            if peripheral.id().to_string() == saved.id && is_connected(peripheral).await {
                return Ok(());
            }
        }
    }

    if let Ok(saved_uuid) = saved.id.parse::<uuid::Uuid>() {
        let central = get_central(state).await?;
        let retrieved = central
            .retrieve_peripherals(RetrievePeripheralsOptions {
                identifiers: Some(vec![PeripheralId::from(saved_uuid)]),
                services: None,
            })
            .await
            .unwrap_or_default();
        if let Some(peripheral) = retrieved.into_iter().next() {
            {
                let mut peripherals = state.inner.peripherals.lock().await;
                peripherals.retain(|known| known.id().to_string() != saved.id);
                peripherals.push(peripheral);
            }
            if connect_device(state, app, &saved.id).await.is_ok() {
                return Ok(());
            }
        }
    }

    let devices = scan_devices_inner(state, Some(&saved.id)).await?;
    if devices.iter().any(|device| device.id == saved.id) {
        return connect_device(state, app, &saved.id).await;
    }
    Err("Previously saved BMS was not found. Move closer and scan again.".to_string())
}

fn spawn_notification_handler(
    state: BatteryState,
    app: AppHandle,
    peripheral: btleplug::platform::Peripheral,
    notify_characteristic: btleplug::api::Characteristic,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut notifications = match with_timeout(
            Duration::from_secs(8),
            "open BMS notification stream",
            peripheral
                .notifications()
                .map_err(|error| format!("could not open BMS notification stream: {error}")),
        )
        .await
        {
            Ok(notifications) => notifications,
            Err(error) => {
                eprintln!("[battery] {error}");
                tracing::warn!("{error}");
                return;
            }
        };
        tracing::info!("notification stream open");
        let mut rx_buffer: Vec<u8> = Vec::new();
        while let Some(notification) = notifications.next().await {
            if notification.uuid != notify_characteristic.uuid {
                eprintln!(
                    "[battery] ignoring notification on {uuid} (expecting {expected})",
                    uuid = notification.uuid,
                    expected = notify_characteristic.uuid,
                );
                continue;
            }
            eprintln!(
                "[battery] notification: {}",
                notification
                    .value
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            rx_buffer.extend_from_slice(&notification.value);
            for frame in drain_complete_frames(&mut rx_buffer) {
                eprintln!(
                    "[battery] complete frame: {}",
                    frame
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                handle_frame(&state, &app, &frame).await;
            }
        }
        tracing::warn!("notification stream ended");
        handle_link_lost(state.clone(), app.clone(), peripheral.id(), "notification stream ended");
    })
}

async fn handle_frame(state: &BatteryState, app: &AppHandle, frame: &[u8]) {
    let command = match frame.get(1) {
        Some(&command) => command,
        None => return,
    };
    *state.inner.last_frame_at.lock().await = Some(Instant::now());
    match command {
        COMMAND_BASIC_STATUS => {
            if let Some(status) = parse_basic_status(frame) {
                eprintln!(
                    "[battery] parsed basic status: {:.2} V / {:.2} A / SOC {}%",
                    status.voltage, status.current, status.soc,
                );
                *state.inner.latest_basic.lock().await = Some(status);
            } else {
                eprintln!("[battery] basic status frame failed to parse");
            }
        }
        COMMAND_CELL_VOLTAGE => {
            if let Some(voltages) = parse_cell_voltages(frame) {
                eprintln!("[battery] parsed cell voltages: {voltages:?}");
                *state.inner.latest_cells.lock().await = voltages;
            } else {
                eprintln!("[battery] cell voltage frame failed to parse");
            }
        }
        other => eprintln!("[battery] unhandled frame command: {other:02x?}"),
    }
    publish_snapshot(state, app).await;
}

async fn publish_snapshot(state: &BatteryState, app: &AppHandle) {
    let basic = state.inner.latest_basic.lock().await.clone();
    let cells = state.inner.latest_cells.lock().await.clone();
    let basic = match basic {
        Some(basic) => basic,
        None => return,
    };
    let meta = state.inner.meta.lock().await;
    let mut snapshot =
        BatterySnapshot::new(meta.device_id.clone(), meta.device_name.clone());
    snapshot.update_from(&basic, &cells);
    drop(meta);
    if let Some(notifier) = tauri::Manager::try_state::<Notifier>(app) {
        notifier.observe(Reading::from_bms(&snapshot)).await;
    }
    *state.inner.latest.lock().await = Some(snapshot.clone());
    events::emit(app, events::BATTERY_SNAPSHOT, snapshot);
}

fn spawn_polling_task(
    state: BatteryState,
    app: AppHandle,
    peripheral: btleplug::platform::Peripheral,
    write_characteristic: btleplug::api::Characteristic,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        tracing::info!("polling task spawned");
        let write_type = if write_characteristic.properties.contains(CharPropFlags::WRITE) {
            WriteType::WithResponse
        } else if write_characteristic
            .properties
            .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE)
        {
            WriteType::WithoutResponse
        } else {
            eprintln!("[battery] BMS write characteristic has no write properties");
            tracing::warn!("BMS write characteristic does not support writing");
            return;
        };
        eprintln!("[battery] polling write type: {write_type:?}");

        loop {
            match tokio::time::timeout(Duration::from_secs(5), peripheral.is_connected()).await {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => {
                    tracing::warn!("polling loop: device no longer connected (OS-reported)");
                    break;
                }
                Ok(Err(error)) => {
                    tracing::warn!("polling loop: connection check failed: {error}");
                }
                Err(_) => {
                    tracing::warn!("polling loop: connection check timed out");
                }
            }
            let stale = state
                .inner
                .last_frame_at
                .lock()
                .await
                .map(|last| last.elapsed() >= STALE_DATA_TIMEOUT)
                .unwrap_or(false);
            if stale {
                tracing::warn!(
                    "no BMS frame received in {}s — link reports connected but data has stopped",
                    STALE_DATA_TIMEOUT.as_secs(),
                );
                break;
            }
            match with_timeout(
                Duration::from_secs(2),
                "BMS status write",
                peripheral
                    .write(&write_characteristic, &STATUS_COMMAND, write_type)
                    .map_err(|error| format!("BMS status write failed: {error}")),
            )
            .await
            {
                Ok(()) => eprintln!("[battery] wrote status command"),
                Err(error) => {
                    eprintln!("[battery] {error}");
                    tracing::warn!("{error}");
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            match with_timeout(
                Duration::from_secs(2),
                "BMS cell voltage write",
                peripheral
                    .write(&write_characteristic, &CELL_VOLTAGE_COMMAND, write_type)
                    .map_err(|error| format!("BMS cell voltage write failed: {error}")),
            )
            .await
            {
                Ok(()) => eprintln!("[battery] wrote cell voltage command"),
                Err(error) => {
                    eprintln!("[battery] {error}");
                    tracing::warn!("{error}");
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        handle_link_lost(state.clone(), app.clone(), peripheral.id(), "polling loop ended");
    })
}