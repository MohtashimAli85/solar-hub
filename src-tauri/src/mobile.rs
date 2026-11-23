use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::Duration;

use futures_util::StreamExt as _;
use serde::Deserialize;
use tauri::{AppHandle, State};

use crate::battery::manager::{disconnect_device, spawn_reconnect_loop};
use crate::battery::BatteryState;
use crate::inverter::client::SolarCredentials;
use crate::inverter::InverterState;
use crate::storage::{
    load_saved_ble_device, save_app_settings, save_saved_ble_device, save_solar_password, AppSettings, SavedBleDevice,
};

const MAC_PORT: u16 = 8787;

fn wifi_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.168.0.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if ip.is_private() => Some(ip),
        _ => None,
    }
}

async fn is_solar_hub(client: &reqwest::Client, base: &str) -> bool {
    let Ok(response) = client.get(format!("{base}/api/session")).send().await else {
        return false;
    };
    match response.status().as_u16() {
        200 => true,
        401 => response.text().await.is_ok_and(|body| body.contains("not paired")),
        _ => false,
    }
}

#[tauri::command]
pub async fn find_mac(preferred: Option<String>) -> Result<Option<String>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(700))
        .timeout(Duration::from_millis(2000))
        .build()
        .map_err(|error| error.to_string())?;

    if let Some(base) = preferred.filter(|base| !base.is_empty()) {
        if is_solar_hub(&client, &base).await {
            return Ok(Some(base));
        }
    }

    let Some(ip) = wifi_ip() else {
        return Ok(None);
    };
    let [a, b, c, own] = ip.octets();
    let candidates: Vec<String> = (1..=254u8)
        .filter(|host| *host != own)
        .map(|host| format!("http://{a}.{b}.{c}.{host}:{MAC_PORT}"))
        .collect();
    let mut probes = futures_util::stream::iter(candidates)
        .map(|base| {
            let client = client.clone();
            async move { is_solar_hub(&client, &base).await.then_some(base) }
        })
        .buffer_unordered(64);
    while let Some(result) = probes.next().await {
        if result.is_some() {
            return Ok(result);
        }
    }
    Ok(None)
}

#[derive(Deserialize)]
pub struct PhoneSetup {
    settings: AppSettings,
    password: String,
    bms_name: Option<String>,
}

#[tauri::command]
pub async fn import_mac_setup(
    app: AppHandle,
    inverter: State<'_, InverterState>,
    setup: PhoneSetup,
) -> Result<(), String> {
    save_app_settings(&app, &setup.settings)?;
    if !setup.password.is_empty() {
        save_solar_password(&setup.password)?;
    }
    inverter
        .solar_client()
        .set_credentials(SolarCredentials {
            user_id: setup.settings.user_id.clone(),
            password: setup.password,
            station_id: setup.settings.station_id.clone(),
            device_id: setup.settings.device_id.clone(),
            time_zone: setup.settings.time_zone.clone(),
        })
        .await;

    if let Some(name) = setup.bms_name {
        let known = load_saved_ble_device(&app).is_some_and(|device| device.name == name);
        if !known {
            save_saved_ble_device(&app, &SavedBleDevice { id: String::new(), name })?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn set_direct_mode(app: AppHandle, battery: State<'_, BatteryState>, on: bool) -> Result<(), String> {
    if on {
        if !battery.connection_status().await.connected {
            spawn_reconnect_loop(battery.inner().clone(), app);
        }
        return Ok(());
    }
    *battery.inner.reconnect_generation.lock().await += 1;
    *battery.inner.reconnecting.lock().await = false;
    if battery.inner.connected.lock().await.is_some() {
        disconnect_device(&battery, &app).await?;
    }
    Ok(())
}

#[no_mangle]
pub extern "system" fn Java_com_mohtashimali_solarhub_MainActivity_initBluetooth<'local>(
    mut env: jni::EnvUnowned<'local>,
    _activity: jni::objects::JObject<'local>,
) {
    let outcome = env.with_env(|env| -> jni::errors::Result<()> {
        if let Err(error) = btleplug::platform::init(env) {
            tracing::warn!("Bluetooth could not start: {error}");
        }
        Ok(())
    });
    let _ = outcome;
}
