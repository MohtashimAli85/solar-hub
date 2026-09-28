use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use reqwest::Method;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;

use super::crypto::{md5_hex, signed_headers, AppIdHeaderMap};
use super::types::{
    DeviceDetails, EnergyFlowData, InverterSettings, InverterSnapshot, charger_mode_name, output_mode_name,
};
use super::InverterError;

const API_BASE: &str = "https://solar.siseli.com";
const ENERGY_FLOW_MAX_AGE: Duration = Duration::from_secs(120);

/// Picks the energy-flow payload to ship: the fresh one, else a cached copy
/// young enough to still describe the house (flagged stale).
fn choose_energy_flow(
    fresh: Option<EnergyFlowData>,
    cached: Option<&(Instant, EnergyFlowData)>,
    now: Instant,
) -> (Option<EnergyFlowData>, bool) {
    if fresh.is_some() {
        return (fresh, false);
    }
    match cached {
        Some((at, data)) if now.saturating_duration_since(*at) <= ENERGY_FLOW_MAX_AGE => (Some(data.clone()), true),
        _ => (None, false),
    }
}

#[derive(Debug, Clone, Default)]
pub struct SolarCredentials {
    pub user_id: String,
    pub password: String,
    pub station_id: String,
    pub device_id: String,
    pub time_zone: String,
}

impl SolarCredentials {
    pub fn time_zone_or_default(&self) -> String {
        if self.time_zone.is_empty() {
            "Asia/Karachi".to_string()
        } else {
            self.time_zone.clone()
        }
    }
}

#[derive(Clone)]
pub struct SolarClient {
    http: reqwest::Client,
    token: Arc<Mutex<Option<String>>>,
    credentials: Arc<Mutex<SolarCredentials>>,
    energy_flow_cache: Arc<Mutex<Option<(Instant, EnergyFlowData)>>>,
}

impl SolarClient {
    pub fn new(credentials: SolarCredentials) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
            token: Arc::new(Mutex::new(None)),
            credentials: Arc::new(Mutex::new(credentials)),
            energy_flow_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_credentials(&self, credentials: SolarCredentials) {
        *self.credentials.lock().await = credentials;
        *self.token.lock().await = None;
    }

    pub async fn credentials(&self) -> SolarCredentials {
        self.credentials.lock().await.clone()
    }

    pub fn is_configured(&self) -> bool {
        !self.credentials.try_lock().map(|c| c.user_id.is_empty() || c.password.is_empty()).unwrap_or(true)
    }

    /// Local time in the configured IANA time zone, formatted the way the Node
    /// `solarTime()` did: `YYYY-MM-DDTHH:MM:SS±HH:MM`.
    pub fn solar_time(&self, instant: DateTime<Utc>) -> Result<String, InverterError> {
        let tz_name = self
            .credentials
            .try_lock()
            .map(|credentials| credentials.time_zone_or_default())
            .unwrap_or_else(|_| "Asia/Karachi".to_string());
        let tz: Tz = tz_name
            .parse()
            .map_err(|_| InverterError::TimeZone("unknown time zone".into()))?;
        let local = tz.from_utc_datetime(&instant.naive_utc());
        let offset_minutes = tz
            .offset_from_utc_datetime(&instant.naive_utc())
            .fix()
            .local_minus_utc()
            / 60;
        let sign = if offset_minutes >= 0 { '+' } else { '-' };
        let absolute = offset_minutes.abs();
        Ok(format!(
            "{}{sign}{:02}:{:02}",
            local.format("%Y-%m-%dT%H:%M:%S"),
            absolute / 60,
            absolute % 60
        ))
    }

    async fn login(&self) -> Result<(), InverterError> {
        let credentials = self.credentials().await;
        eprintln!(
            "[inverter] login attempt for user_id={}",
            credentials.user_id
        );
        if credentials.user_id.is_empty() || credentials.password.is_empty() {
            return Err(InverterError::Config(
                "Set a Solar user ID and password in Settings".into(),
            ));
        }
        let password = md5_hex(credentials.password.as_bytes());
        let body = json!({ "account": credentials.user_id, "password": password }).to_string();
        eprintln!("[inverter] login request: {body}");
        let headers = signed_headers(&body)?;
        let response = self
            .http
            .post(format!("{API_BASE}/apis/login/account"))
            .headers(AppIdHeaderMap::from_map(headers))
            .body(body)
            .send()
            .await?;
        eprintln!("[inverter] login HTTP status {}", response.status());
        let data: Value = response.json().await.map_err(|_| {
            eprintln!("[inverter] login response was not valid JSON");
            InverterError::Api("login response was not valid JSON".into())
        })?;
        eprintln!("[inverter] login response: {data}");
        ensure_code_ok(&data)?;
        let payload = data.get("data").cloned().unwrap_or_else(|| data.clone());
        let token = payload
            .get("accessToken")
            .and_then(Value::as_str)
            .or_else(|| payload.get("iotToken").and_then(Value::as_str))
            .or_else(|| payload.get("token").and_then(Value::as_str))
            .ok_or_else(|| InverterError::Auth("login response did not contain an access token".into()))?;
        *self.token.lock().await = Some(token.to_string());
        Ok(())
    }

    async fn solar_request(
        &self,
        endpoint: &str,
        method: Method,
        body: Value,
    ) -> Result<Value, InverterError> {
        let mut retried = false;
        loop {
            let cached_token = self.token.lock().await.clone();
            let token = match cached_token {
                Some(token) => token,
                None => {
                    self.login().await?;
                    continue;
                }
            };
            let time_zone = self.credentials().await.time_zone_or_default();
            let body_string = body.to_string();
            let mut request = self
                .http
                .request(method.clone(), format!("{API_BASE}{endpoint}"))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json; charset=utf-8")
                .header("Origin", "https://solar.siseli.com")
                .header("Referer", "https://solar.siseli.com/")
                .header("IOT-Token", token)
                .header("IOT-Time-Zone", time_zone);
            if method != Method::GET {
                request = request.body(body_string);
            }
            eprintln!("[inverter] sending {method} {endpoint}");
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    eprintln!("[inverter] {endpoint} request failed to send: {error}");
                    return Err(InverterError::Api(format!(
                        "{endpoint} request failed to send: {error}"
                    )));
                }
            };
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                *self.token.lock().await = None;
                if !retried {
                    retried = true;
                    continue;
                }
                return Err(InverterError::Auth(
                    "unable to re-authenticate after a 401".into(),
                ));
            }
            let status = response.status();
            let data: Value = response.json().await.map_err(|_| {
                eprintln!("[inverter] {endpoint} returned non-JSON response (status {status})");
                InverterError::Api(format!("{endpoint} returned non-JSON response"))
            })?;
            eprintln!("[inverter] {endpoint} response: {data}");
            if is_token_expired(&data) {
                eprintln!("[inverter] {endpoint} token expired (code 9), clearing token");
                *self.token.lock().await = None;
                if !retried {
                    retried = true;
                    continue;
                }
                return Err(InverterError::Auth(
                    "unable to re-authenticate after token expiry".into(),
                ));
            }
            ensure_code_ok(&data)?;
            return Ok(data);
        }
    }

    pub async fn read_inverter_snapshot(
        &self,
        device_id: Option<String>,
    ) -> Result<InverterSnapshot, InverterError> {
        let device_id = match device_id {
            Some(id) if !id.trim().is_empty() => id,
            _ => self.credentials().await.device_id,
        };
        eprintln!("[inverter] read_inverter_snapshot device_id={device_id:?}");
        if device_id.is_empty() {
            eprintln!("[inverter] snapshot aborted: no device_id set");
            return Err(InverterError::Config(
                "Set a Solar device ID in Settings".into(),
            ));
        }
        let data = self
            .solar_request(
                &format!(
                    "/apis/deviceState/simple/state/latest/v1?deviceId={}&dataSource=1",
                    urlencode(&device_id)
                ),
                Method::GET,
                Value::Null,
            )
            .await?;
        let payload = data.get("data").cloned().unwrap_or_else(|| json!({}));
        let mut fields = payload
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| serde_json::Map::new());
        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(Vec::new);
        let firing_alarms = payload
            .get("firingAlarms")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(Vec::new);
        let settings = self.read_inverter_settings(&device_id).await.ok();
        let (energy_flow, energy_flow_stale) = self.energy_flow_with_fallback(&device_id).await;
        let mut pv_panel_flow = None;
        let mut grid_flow = None;
        let mut load_flow = None;

        if let Some(energy_flow) = energy_flow {
            fields.extend(energy_flow.fields);
            pv_panel_flow = energy_flow.pv_panel_flow;
            grid_flow = energy_flow.grid_flow;
            load_flow = energy_flow.load_flow;
        }

        let mut snapshot = InverterSnapshot {
            device_id,
            fields,
            groups,
            firing_alarms,
            settings,
            pv_panel_flow,
            grid_flow,
            load_flow,
            energy_flow_stale,
            grid: None,
        };
        snapshot.grid = Some(snapshot.grid_status(None, None));
        Ok(snapshot)
    }

    async fn energy_flow_with_fallback(&self, device_id: &str) -> (Option<EnergyFlowData>, bool) {
        let mut fresh = None;
        for attempt in 1..=2 {
            if attempt > 1 {
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
            match self.read_energy_flow_fields(device_id).await {
                Ok(data) => {
                    fresh = Some(data);
                    break;
                }
                Err(error) => tracing::warn!("energy-flow request failed (attempt {attempt}/2): {error}"),
            }
        }
        let mut cache = self.energy_flow_cache.lock().await;
        if let Some(data) = &fresh {
            *cache = Some((Instant::now(), data.clone()));
        }
        choose_energy_flow(fresh, cache.as_ref(), Instant::now())
    }

    async fn read_energy_flow_fields(
        &self,
        device_id: &str,
    ) -> Result<EnergyFlowData, InverterError> {
        let data = self
            .solar_request(
                &format!(
                    "/apis/deviceState/simple/energy/flow/v1?deviceId={}&dataSource=2",
                    urlencode(device_id)
                ),
                Method::GET,
                Value::Null,
            )
            .await?;
        let payload = data.get("data").cloned().unwrap_or_else(|| json!({}));
        let state = payload
            .get("deviceAttributeState")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let fields = state
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .or_else(|| payload.get("fields").and_then(Value::as_object).cloned())
            .unwrap_or_else(|| serde_json::Map::new());

        let pv_panel_flow = pick_flow(&payload, &state, &fields, &["pvPanelFlow", "pv_panel_flow", "pvFlow", "pv_flow"]);
        let grid_flow = pick_flow(&payload, &state, &fields, &["gridFlow", "grid_flow"]);
        let load_flow = pick_flow(&payload, &state, &fields, &["loadFlow", "load_flow"]);

        Ok(EnergyFlowData {
            fields,
            pv_panel_flow,
            grid_flow,
            load_flow,
        })
    }

    pub async fn read_inverter_settings(
        &self,
        device_id: &str,
    ) -> Result<InverterSettings, InverterError> {
        if device_id.is_empty() {
            return Err(InverterError::Config(
                "Set a Solar device ID in Settings".into(),
            ));
        }
        let data = self
            .solar_request(
                &format!(
                    "/apis/remote/device/configs/cache/get?deviceId={}",
                    urlencode(device_id)
                ),
                Method::POST,
                json!({}),
            )
            .await?;
        let settings = data.get("data").cloned().unwrap_or_else(|| json!({}));
        let value_of = |entry: &Value| -> Value {
            match entry {
                Value::Object(map) => map
                    .get("value")
                    .cloned()
                    .unwrap_or_else(|| Value::Null),
                other => other.clone(),
            }
        };
        let setting_value = |key: &str| -> Value {
            value_of(&settings.get(key).cloned().unwrap_or_else(|| Value::Null))
        };
        let output = setting_value("outputSourcePrioritySetting");
        let charger = setting_value("chargerSourcePrioritySetting");
        let f64_setting = |keys: &[&str]| -> Option<f64> {
            keys.iter()
                .find_map(|key| parse_f64(&setting_value(key)))
        };
        Ok(InverterSettings {
            output_source_priority: parse_mode(&output, output_mode_name),
            output_source_priority_value: parse_uint(&output),
            charger_source_priority: parse_mode(&charger, charger_mode_name),
            charger_source_priority_value: parse_uint(&charger),
            ac_input_range: parse_uint(&setting_value("acInputRangeSetting")),
            battery_power_limiting: parse_uint(&setting_value("batteryPowerLimitingSetting")),
            low_battery_cutoff_voltage: f64_setting(&[
                "LowBatteryCutOffVoltageSetting",
                "lowBatteryCutOffVoltageSetting",
            ]),
            high_cutoff_voltage: f64_setting(&["highCutOffVoltageSetting"]),
            low_dc_cutoff_soc: f64_setting(&["lowDCCutOffSOCSetting"]),
            max_total_charge_current: f64_setting(&["maxTotalChargeCurrentSetting"]),
            max_utility_charge_current: f64_setting(&["maxUtilityChargeCurrentSetting"]),
            smart_load: parse_uint(&setting_value("smartLoadSetting")),
        })
    }

    async fn resolve_device_id(&self, device_id: Option<String>) -> Result<String, InverterError> {
        let device_id = match device_id {
            Some(id) if !id.trim().is_empty() => id,
            _ => self.credentials().await.device_id,
        };
        if device_id.is_empty() {
            return Err(InverterError::Config(
                "Set a Solar device ID in Settings".into(),
            ));
        }
        Ok(device_id)
    }

    async fn write_setting(
        &self,
        device_id: &str,
        key: &str,
        value: String,
    ) -> Result<(), InverterError> {
        let body = json!({
            "id": device_id,
            "key": key,
            "value": value,
        });
        self.solar_request(
            &format!(
                "/apis/remote/device/config/write?deviceId={}",
                urlencode(device_id)
            ),
            Method::POST,
            body,
        )
        .await?;
        Ok(())
    }

    pub async fn write_output_priority(
        &self,
        device_id: Option<String>,
        mode: String,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        let values: BTreeMap<&str, u32> = BTreeMap::from([
            ("solar", 0),
            ("sbg", 1),
            ("utility", 2),
        ]);
        let value = match mode.parse::<u32>() {
            Ok(number) if (0..=2).contains(&number) => number,
            _ => *values.get(mode.trim().to_lowercase().as_str()).ok_or_else(|| {
                InverterError::Api(format!(
                    "value/mode must be 0, 1, 2 or utility, solar, sbg (got {mode})"
                ))
            })?,
        };
        self.write_setting(&device_id, "outputSourcePrioritySetting", value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    pub async fn write_charger_priority(
        &self,
        device_id: Option<String>,
        mode: String,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        let values: BTreeMap<&str, u32> = BTreeMap::from([
            ("solar", 0),
            ("solar or utility", 1),
            ("solar only", 2),
        ]);
        let value = match mode.parse::<u32>() {
            Ok(number) if (0..=2).contains(&number) => number,
            _ => *values.get(mode.trim().to_lowercase().as_str()).ok_or_else(|| {
                InverterError::Api(format!(
                    "value/mode must be 0, 1, 2 or solar, solar or utility, solar only (got {mode})"
                ))
            })?,
        };
        self.write_setting(&device_id, "chargerSourcePrioritySetting", value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    pub async fn write_ac_input_range(
        &self,
        device_id: Option<String>,
        enabled: bool,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        let value = if enabled { "0" } else { "1" };
        self.write_setting(&device_id, "acInputRangeSetting", value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    async fn write_f64_setting(
        &self,
        device_id: Option<String>,
        key: &str,
        value: f64,
        range: std::ops::RangeInclusive<f64>,
        range_label: &str,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        if !range.contains(&value) {
            return Err(InverterError::Api(format!(
                "value must be {range_label} (got {value})"
            )));
        }
        self.write_setting(device_id.as_str(), key, value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    pub async fn write_low_battery_cutoff_voltage(
        &self,
        device_id: Option<String>,
        value: f64,
    ) -> Result<InverterSettings, InverterError> {
        self.write_f64_setting(
            device_id,
            "LowBatteryCutOffVoltageSetting",
            value,
            0.0..=100.0,
            "0-100 V",
        )
        .await
    }

    pub async fn write_high_cutoff_voltage(
        &self,
        device_id: Option<String>,
        value: f64,
    ) -> Result<InverterSettings, InverterError> {
        self.write_f64_setting(
            device_id,
            "highCutOffVoltageSetting",
            value,
            0.0..=100.0,
            "0-100 V",
        )
        .await
    }

    pub async fn write_low_dc_cutoff_soc(
        &self,
        device_id: Option<String>,
        value: f64,
    ) -> Result<InverterSettings, InverterError> {
        self.write_f64_setting(
            device_id,
            "lowDCCutOffSOCSetting",
            value,
            0.0..=100.0,
            "0-100 %",
        )
        .await
    }

    pub async fn write_max_total_charge_current(
        &self,
        device_id: Option<String>,
        value: f64,
    ) -> Result<InverterSettings, InverterError> {
        self.write_f64_setting(
            device_id,
            "maxTotalChargeCurrentSetting",
            value,
            0.0..=1000.0,
            "0-1000 A",
        )
        .await
    }

    pub async fn write_max_utility_charge_current(
        &self,
        device_id: Option<String>,
        value: f64,
    ) -> Result<InverterSettings, InverterError> {
        self.write_f64_setting(
            device_id,
            "maxUtilityChargeCurrentSetting",
            value,
            0.0..=1000.0,
            "0-1000 A",
        )
        .await
    }

    pub async fn write_grid_feed_in(
        &self,
        device_id: Option<String>,
        enabled: bool,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        let value = if enabled { "1" } else { "0" };
        self.write_setting(&device_id, "batteryPowerLimitingSetting", value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    pub async fn write_smart_load(
        &self,
        device_id: Option<String>,
        enabled: bool,
    ) -> Result<InverterSettings, InverterError> {
        let device_id = self.resolve_device_id(device_id).await?;
        let value = if enabled { "1" } else { "0" };
        self.write_setting(&device_id, "smartLoadSetting", value.to_string())
            .await?;
        self.read_inverter_settings(&device_id).await
    }

    pub async fn read_device_details(
        &self,
        device_id: Option<String>,
    ) -> Result<DeviceDetails, InverterError> {
        let device_id = match device_id {
            Some(id) if !id.trim().is_empty() => id,
            _ => self.credentials().await.device_id,
        };
        if device_id.is_empty() {
            return Err(InverterError::Config(
                "Set a Solar device ID in Settings".into(),
            ));
        }
        let data = self
            .solar_request(
                &format!(
                    "/apis/device/details?deviceId={}",
                    urlencode(&device_id)
                ),
                Method::GET,
                Value::Null,
            )
            .await?;
        let device = data.get("data").cloned().unwrap_or_else(|| json!({}));
        let number = |key: &str| device.get(key).and_then(parse_f64);
        Ok(DeviceDetails {
            is_online: device
                .get("isOnline")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rated_power: number("ratedPower"),
            daily_produced_quantity: number("dailyProducedQuantity"),
            total_produced_quantity: number("totalProducedQuantity"),
            state_dict: device
                .get("stateDict")
                .and_then(Value::as_str)
                .map(str::to_string),
            station_name: device
                .get("stationName")
                .and_then(Value::as_str)
                .map(str::to_string),
            co2_emission_reduction: number("co2EmissionReduction"),
            so2_emission_reduction: number("so2EmissionReduction"),
            nox_emission_reduction: number("noxEmissionReduction"),
        })
    }
}

fn ensure_code_ok(data: &Value) -> Result<(), InverterError> {
    match data.get("code") {
        None | Some(Value::Null) => Ok(()),
        Some(code) => {
            let ok = code.as_i64() == Some(0) || code.as_str() == Some("0");
            if ok {
                Ok(())
            } else {
                let message = data
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                Err(InverterError::Api(format!(
                    "code {}: {message}",
                    code.as_i64().unwrap_or(0)
                )))
            }
        }
    }
}

fn is_token_expired(data: &Value) -> bool {
    data.get("code").and_then(Value::as_i64) == Some(9)
}

fn parse_mode(value: &Value, naming: fn(u32) -> String) -> String {
    match parse_uint(value) {
        Some(number) => naming(number),
        None => "Unknown".to_string(),
    }
}

fn parse_uint(value: &Value) -> Option<u32> {
    value
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .or_else(|| value.as_u64().map(|v| v as u32))
}

fn parse_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))
        .filter(|number| number.is_finite())
}

fn pick_flow(
    payload: &Value,
    state: &Value,
    fields: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<Value> {
    for key in keys {
        if let Some(flow) = payload.get(*key) {
            if !flow.is_null() {
                return Some(flow.clone());
            }
        }
        if let Some(flow) = state.get(*key) {
            if !flow.is_null() {
                return Some(flow.clone());
            }
        }
        if let Some(flow) = fields.get(*key) {
            if !flow.is_null() {
                return Some(flow.clone());
            }
        }
    }
    None
}

fn urlencode(value: &str) -> String {
    use reqwest::Url;
    Url::parse("http://localhost")
        .unwrap()
        .join(&format!("/?v={}", value))
        .map(|u| {
            u.query()
                .unwrap_or_default()
                .split_once('=')
                .map(|(_, v)| v.to_string())
                .unwrap_or_default()
        })
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow_data() -> EnergyFlowData {
        EnergyFlowData {
            fields: serde_json::Map::new(),
            pv_panel_flow: None,
            grid_flow: Some(json!({"value": 420})),
            load_flow: None,
        }
    }

    #[test]
    fn fresh_energy_flow_wins() {
        let now = Instant::now();
        let (data, stale) = choose_energy_flow(Some(flow_data()), None, now);
        assert!(data.is_some());
        assert!(!stale);
    }

    #[test]
    fn recent_cache_fills_a_failed_request_as_stale() {
        let now = Instant::now();
        let cached = (now - Duration::from_secs(60), flow_data());
        let (data, stale) = choose_energy_flow(None, Some(&cached), now);
        assert!(data.is_some());
        assert!(stale);
    }

    #[test]
    fn old_cache_is_not_reused() {
        let now = Instant::now();
        let cached = (now - Duration::from_secs(300), flow_data());
        let (data, stale) = choose_energy_flow(None, Some(&cached), now);
        assert!(data.is_none());
        assert!(!stale);
    }

    #[test]
    fn solar_time_formats_with_offset() {
        let client = SolarClient::new(SolarCredentials {
            time_zone: "Asia/Karachi".to_string(),
            ..Default::default()
        });
        let instant = DateTime::parse_from_rfc3339("2026-09-23T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let formatted = client.solar_time(instant).unwrap();
        assert!(formatted.starts_with("2026-09-23T05:00:00+05:00"));
    }

    #[test]
    fn code_ok_accepts_zero_variants() {
        assert!(ensure_code_ok(&json!({"code": 0, "message": "ok"})).is_ok());
        assert!(ensure_code_ok(&json!({"code": "0"})).is_ok());
        assert!(ensure_code_ok(&json!({"foo": 1})).is_ok());
        assert!(ensure_code_ok(&json!({"code": null})).is_ok());
        assert!(ensure_code_ok(&json!({"code": 1, "message": "bad"})).is_err());
    }

    #[test]
    fn token_expired_detected_by_code_9() {
        assert!(is_token_expired(&json!({"code": 9, "message": "Token expired"})));
        assert!(!is_token_expired(&json!({"code": 0})));
        assert!(!is_token_expired(&json!({"code": 1, "message": "bad"})));
        assert!(!is_token_expired(&json!({"foo": 9})));
    }
}