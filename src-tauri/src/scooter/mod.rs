pub mod protocol;

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{Central as _, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use chrono::{Local, NaiveDateTime, Timelike};
use futures_util::{Stream, StreamExt as _, TryFutureExt as _};
use serde::Serialize;
use tokio::sync::Mutex;
use uuid::Uuid;

use protocol::{
    drain_frames, hex, is_pairing_accepted, mac_from_serial, pairing_packet, parse_frame, parse_mac, power_packet, Frame,
    COMMAND_UUID, FRAME_UUID, HANDSHAKE_REPLY_UUID, HANDSHAKE_UUID, SERIAL_UUID,
};

const SCAN_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const STEP_TIMEOUT: Duration = Duration::from_secs(5);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE_FRAMES: Duration = Duration::from_secs(3);
/// SOC only updates while the scooter is on, so the frames after ON need a
/// few seconds before the value is live.
const ON_FRAMES: Duration = Duration::from_secs(8);
const AFTER_OFF_FRAMES: Duration = Duration::from_secs(3);
const OFF_TRIES: usize = 3;
const OFF_RETRY_PAUSE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScooterReading {
    pub soc: Option<f64>,
    pub read_at: NaiveDateTime,
    pub power_cycled: bool,
    pub off_sent: bool,
    pub state_before: Option<String>,
    pub state_on: Option<String>,
    pub state_after: Option<String>,
}

struct Inner {
    latest: Mutex<Option<ScooterReading>>,
    last_error: Mutex<Option<String>>,
    op_lock: Mutex<()>,
    busy: AtomicBool,
}

#[derive(Clone)]
pub struct ScooterState {
    inner: Arc<Inner>,
}

impl Default for ScooterState {
    fn default() -> Self {
        Self::new()
    }
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl ScooterState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                latest: Mutex::new(None),
                last_error: Mutex::new(None),
                op_lock: Mutex::new(()),
                busy: AtomicBool::new(false),
            }),
        }
    }

    pub async fn latest(&self) -> Option<ScooterReading> {
        self.inner.latest.lock().await.clone()
    }

    pub async fn last_error(&self) -> Option<String> {
        self.inner.last_error.lock().await.clone()
    }

    pub async fn busy(&self) -> bool {
        self.inner.busy.load(Ordering::SeqCst)
    }

    /// Serialized by an internal op lock; stores latest / last_error.
    pub async fn read(&self, name: &str, mac: Option<&str>, power_cycle: bool) -> Result<ScooterReading, String> {
        let _op = self.inner.op_lock.lock().await;
        self.inner.busy.store(true, Ordering::SeqCst);
        let _busy = BusyGuard(&self.inner.busy);
        let result = read_once(name, mac, power_cycle).await;
        match &result {
            Ok(reading) => {
                tracing::info!("[scooter] read {:?}% (power cycled: {})", reading.soc, reading.power_cycled);
                *self.inner.latest.lock().await = Some(reading.clone());
                *self.inner.last_error.lock().await = None;
            }
            Err(error) => {
                tracing::warn!("[scooter] read failed: {error}");
                *self.inner.last_error.lock().await = Some(error.clone());
            }
        }
        result
    }
}

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

async fn read_once(name: &str, mac: Option<&str>, power_cycle: bool) -> Result<ScooterReading, String> {
    let mac = mac
        .map(|text| parse_mac(text).ok_or_else(|| format!("scooter MAC \"{text}\" isn't six hex bytes")))
        .transpose()?;
    let adapter = adapter().await?;
    let peripheral = find(&adapter, name).await?;
    let result = session(&peripheral, mac, power_cycle).await;
    let _ = with_timeout(STEP_TIMEOUT, "disconnect scooter", peripheral.disconnect().map_err(|error| error.to_string())).await;
    result
}

async fn adapter() -> Result<Adapter, String> {
    let manager = with_timeout(
        Duration::from_secs(8),
        "create Bluetooth manager",
        Manager::new().map_err(|error| error.to_string()),
    )
    .await?;
    with_timeout(Duration::from_secs(8), "list Bluetooth adapters", async {
        manager
            .adapters()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "No Bluetooth adapter was found on this Mac".to_string())
    })
    .await
}

async fn has_name(peripheral: &Peripheral, name: &str) -> bool {
    peripheral.properties().await.ok().flatten().and_then(|properties| properties.local_name).as_deref() == Some(name)
}

async fn find(adapter: &Adapter, name: &str) -> Result<Peripheral, String> {
    let mut events = with_timeout(
        STEP_TIMEOUT,
        "subscribe to Bluetooth scan events",
        adapter.events().map_err(|error| error.to_string()),
    )
    .await?;
    with_timeout(
        STEP_TIMEOUT,
        "start Bluetooth scan",
        adapter.start_scan(ScanFilter::default()).map_err(|error| format!("unable to start Bluetooth scan: {error}")),
    )
    .await?;
    let search = async {
        for peripheral in adapter.peripherals().await.unwrap_or_default() {
            if has_name(&peripheral, name).await {
                return Some(peripheral);
            }
        }
        while let Some(event) = events.next().await {
            if let CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) = event {
                if let Ok(peripheral) = adapter.peripheral(&id).await {
                    if has_name(&peripheral, name).await {
                        return Some(peripheral);
                    }
                }
            }
        }
        None
    };
    let found = tokio::time::timeout(SCAN_TIMEOUT, search).await.ok().flatten();
    let _ = with_timeout(STEP_TIMEOUT, "stop Bluetooth scan", adapter.stop_scan().map_err(|error| error.to_string())).await;
    found.ok_or_else(|| format!("scooter {name} wasn't found over Bluetooth — is it within range of the Mac?"))
}

struct Notifications {
    stream: Pin<Box<dyn Stream<Item = ValueNotification> + Send>>,
    buffer: Vec<u8>,
}

impl Notifications {
    async fn wait_pairing(&mut self) -> Result<(), String> {
        let wait = async {
            while let Some(notification) = self.stream.next().await {
                if notification.uuid == HANDSHAKE_REPLY_UUID {
                    return if is_pairing_accepted(&notification.value) {
                        Ok(())
                    } else {
                        Err(format!("the scooter rejected the pairing ({})", hex(&notification.value)))
                    };
                }
                if notification.uuid == FRAME_UUID {
                    self.buffer.extend(notification.value);
                }
            }
            Err("the scooter's notification stream ended".to_string())
        };
        tokio::time::timeout(PAIRING_TIMEOUT, wait)
            .await
            .unwrap_or_else(|_| Err("the scooter didn't answer the pairing".to_string()))
    }

    async fn frames_for(&mut self, duration: Duration, label: &str) -> Vec<Frame> {
        let deadline = tokio::time::Instant::now() + duration;
        let mut frames = Vec::new();
        let (mut first, mut last) = (None, None);
        while let Ok(Some(notification)) = tokio::time::timeout_at(deadline, self.stream.next()).await {
            if notification.uuid != FRAME_UUID {
                continue;
            }
            self.buffer.extend(notification.value);
            for raw in drain_frames(&mut self.buffer) {
                if let Some(frame) = parse_frame(&raw) {
                    frames.push(frame);
                    first.get_or_insert_with(|| hex(&raw));
                    last = Some(hex(&raw));
                } else {
                    tracing::debug!("[scooter] {label}: dropped a frame that failed its checksum: {}", hex(&raw));
                }
            }
        }
        tracing::info!("[scooter] {label}: {} frames, first: {first:?}, last: {last:?}", frames.len());
        frames
    }
}

fn characteristic(peripheral: &Peripheral, uuid: Uuid) -> Result<Characteristic, String> {
    peripheral
        .characteristics()
        .into_iter()
        .find(|characteristic| characteristic.uuid == uuid)
        .ok_or_else(|| format!("the scooter doesn't expose characteristic {uuid}"))
}

async fn write(peripheral: &Peripheral, characteristic: &Characteristic, value: &[u8], label: &str) -> Result<(), String> {
    with_timeout(
        STEP_TIMEOUT,
        label,
        peripheral.write(characteristic, value, WriteType::WithResponse).map_err(|error| error.to_string()),
    )
    .await
}

async fn read(peripheral: &Peripheral, characteristic: &Characteristic, label: &str) -> Result<Vec<u8>, String> {
    with_timeout(STEP_TIMEOUT, label, peripheral.read(characteristic).map_err(|error| error.to_string())).await
}

fn last_soc(frames: &[Frame]) -> Option<f64> {
    frames.iter().rev().find_map(|frame| frame.soc1)
}

fn state_hex(frames: &[Frame]) -> Option<String> {
    frames.last().map(|frame| hex(&frame.state_bytes))
}

async fn switch_off(peripheral: &Peripheral, command: &Characteristic) -> bool {
    for attempt in 1..=OFF_TRIES {
        match write(peripheral, command, &power_packet(false), "switch scooter off").await {
            Ok(()) => return true,
            Err(error) => {
                tracing::warn!("[scooter] OFF attempt {attempt}/{OFF_TRIES} failed: {error}");
                tokio::time::sleep(OFF_RETRY_PAUSE).await;
            }
        }
    }
    false
}

async fn session(peripheral: &Peripheral, mac: Option<[u8; 6]>, power_cycle: bool) -> Result<ScooterReading, String> {
    with_timeout(
        CONNECT_TIMEOUT,
        "connect to scooter",
        peripheral.connect().map_err(|error| format!("connection failed: {error}")),
    )
    .await?;
    with_timeout(
        CONNECT_TIMEOUT,
        "discover scooter services",
        peripheral.discover_services().map_err(|error| format!("service discovery failed: {error}")),
    )
    .await?;
    let handshake = characteristic(peripheral, HANDSHAKE_UUID)?;
    let reply = characteristic(peripheral, HANDSHAKE_REPLY_UUID)?;
    let status = characteristic(peripheral, FRAME_UUID)?;
    let command = characteristic(peripheral, COMMAND_UUID)?;

    let stream = with_timeout(
        STEP_TIMEOUT,
        "open scooter notifications",
        peripheral.notifications().map_err(|error| error.to_string()),
    )
    .await?;
    let mut notifications = Notifications { stream, buffer: Vec::new() };
    for target in [&reply, &status] {
        with_timeout(
            STEP_TIMEOUT,
            "subscribe to scooter notifications",
            peripheral.subscribe(target).map_err(|error| error.to_string()),
        )
        .await?;
    }

    let mac = match mac {
        Some(mac) => mac,
        None => {
            let serial = read(peripheral, &characteristic(peripheral, SERIAL_UUID)?, "read scooter serial").await?;
            mac_from_serial(&serial).ok_or_else(|| format!("the scooter's serial ({}) doesn't hold a MAC", hex(&serial)))?
        }
    };
    let challenge = read(peripheral, &handshake, "read scooter challenge").await?;
    if challenge.len() < 3 {
        return Err(format!("the scooter's challenge was too short ({})", hex(&challenge)));
    }
    let now = Local::now();
    let packet = pairing_packet(&challenge, now.hour(), now.minute(), now.second(), mac);
    write(peripheral, &handshake, &packet, "write scooter pairing").await?;
    notifications.wait_pairing().await?;

    let idle = notifications.frames_for(IDLE_FRAMES, "idle").await;
    if idle.is_empty() {
        return Err("the scooter paired but sent no status frames".to_string());
    }
    let state_before = state_hex(&idle);
    if !power_cycle {
        return Ok(ScooterReading {
            soc: last_soc(&idle),
            read_at: Local::now().naive_local(),
            power_cycled: false,
            off_sent: false,
            state_before,
            state_on: None,
            state_after: None,
        });
    }
    if idle.iter().any(|frame| frame.speed.is_some_and(|speed| speed > 0.0)) {
        return Err("scooter is moving".to_string());
    }

    let on = write(peripheral, &command, &power_packet(true), "switch scooter on").await;
    let on_frames = match on {
        Ok(()) => notifications.frames_for(ON_FRAMES, "on").await,
        Err(_) => Vec::new(),
    };
    let read_at = Local::now().naive_local();
    let off_sent = switch_off(peripheral, &command).await;
    let after = notifications.frames_for(AFTER_OFF_FRAMES, "after off").await;
    match (on, off_sent) {
        (_, false) => Err("could not switch the scooter OFF after reading it — it may still be ON".to_string()),
        (Err(error), true) => Err(format!("could not switch the scooter on to read it: {error}")),
        (Ok(()), true) => Ok(ScooterReading {
            soc: last_soc(&on_frames),
            read_at,
            power_cycled: true,
            off_sent: true,
            state_before,
            state_on: state_hex(&on_frames),
            state_after: state_hex(&after),
        }),
    }
}
