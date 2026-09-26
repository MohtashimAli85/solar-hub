use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub struct EnergyFlowData {
    pub fields: Map<String, Value>,
    pub pv_panel_flow: Option<Value>,
    pub grid_flow: Option<Value>,
    pub load_flow: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InverterSettings {
    pub output_source_priority: String,
    pub output_source_priority_value: Option<u32>,
    pub charger_source_priority: String,
    pub charger_source_priority_value: Option<u32>,
    pub ac_input_range: Option<u32>,
    pub battery_power_limiting: Option<u32>,
    pub low_battery_cutoff_voltage: Option<f64>,
    pub high_cutoff_voltage: Option<f64>,
    pub low_dc_cutoff_soc: Option<f64>,
    pub max_total_charge_current: Option<f64>,
    pub max_utility_charge_current: Option<f64>,
    pub smart_load: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDetails {
    pub is_online: bool,
    pub rated_power: Option<f64>,
    pub daily_produced_quantity: Option<f64>,
    pub total_produced_quantity: Option<f64>,
    pub state_dict: Option<String>,
    pub station_name: Option<String>,
    pub co2_emission_reduction: Option<f64>,
    pub so2_emission_reduction: Option<f64>,
    pub nox_emission_reduction: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InverterSnapshot {
    pub device_id: String,
    pub fields: Map<String, Value>,
    pub groups: Vec<Value>,
    pub firing_alarms: Vec<Value>,
    pub settings: Option<InverterSettings>,
    pub pv_panel_flow: Option<Value>,
    pub grid_flow: Option<Value>,
    pub load_flow: Option<Value>,
}

impl InverterSnapshot {
    pub fn field(&self, names: &[&str]) -> Option<f64> {
        for name in names {
            let raw = self.fields.get(*name)?;
            let item = raw.as_array().and_then(|array| array.last()).unwrap_or(raw);
            if let Some(number) = item.get("value") {
                if let Some(text) = number.as_str() {
                    if let Ok(parsed) = text.parse::<f64>() {
                        if parsed.is_finite() {
                            return Some(parsed);
                        }
                    }
                } else if let Some(parsed) = number.as_f64() {
                    if parsed.is_finite() {
                        return Some(parsed);
                    }
                }
            } else if let Some(parsed) = item.as_f64() {
                if parsed.is_finite() {
                    return Some(parsed);
                }
            }
        }
        None
    }

    pub fn flow_watts(flow: &Option<Value>) -> Option<f64> {
        let flow = flow.as_ref()?;
        let (raw_value, unit) = match flow.get("value") {
            Some(Value::Object(inner)) => (
                inner.get("value").cloned().unwrap_or(Value::Null),
                inner.get("unit").and_then(Value::as_str).unwrap_or(""),
            ),
            Some(other) => (
                other.clone(),
                flow.get("unit").and_then(Value::as_str).unwrap_or(""),
            ),
            None => (Value::Null, ""),
        };
        let raw = raw_value
            .as_f64()
            .or_else(|| raw_value.as_str().and_then(|s| s.parse::<f64>().ok()))?;
        if !raw.is_finite() {
            return None;
        }
        Some(if unit.eq_ignore_ascii_case("kW") {
            raw * 1000.0
        } else {
            raw
        })
    }

    /// Current PV output in watts, from the energy-flow payload first.
    pub fn pv_watts(&self) -> Option<f64> {
        Self::flow_watts(&self.pv_panel_flow)
            .or_else(|| self.field(&["pvInputPower", "generationPower", "pvPower"]))
    }

    /// Current load in watts. Prefers the energy-flow payload, then falls back
    /// to the plain `outputActivePower` / `acOutputActivePower` fields.
    pub fn load_watts(&self) -> Option<f64> {
        Self::flow_watts(&self.load_flow)
            .or_else(|| self.field(&["outputActivePower", "acOutputActivePower"]))
    }

    /// Whether the grid input is up, assuming mains is present once the AC
    /// input voltage crosses 100 V.
    pub fn grid_on(&self) -> Option<bool> {
        self.field(&["acInputVoltage"]).map(|voltage| voltage >= 100.0)
    }
}

pub fn output_mode_name(value: u32) -> String {
    ["Solar", "SBG", "Utility"]
        .get(value as usize)
        .map_or("Unknown".to_string(), |name| name.to_string())
}

pub fn charger_mode_name(value: u32) -> String {
    ["Solar", "Solar or Utility", "Solar only"]
        .get(value as usize)
        .map_or("Unknown".to_string(), |name| name.to_string())
}