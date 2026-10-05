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

pub const SOLAR_OUTPUT_MODE: u32 = 0;
const SOLAR_MODE_DISCHARGE_LIMIT_A: f64 = 1.0;
pub(crate) const MAINS_PRESENT_VOLTS: f64 = 100.0;
const LOAD_OVER_PV_MARGIN_W: f64 = 100.0;
/// Share of the house's shortfall (load minus solar) the battery must carry
/// before a Solar-mode draw counts as an outage; a small trickle while the
/// grid is up is just the inverter's own behaviour.
const OUTAGE_BATTERY_SHARE: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GridBasis {
    BatteryDrawInSolarMode,
    AcInput,
    LoadWithoutBattery,
    Unknown,
}

impl GridBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            GridBasis::BatteryDrawInSolarMode => "battery_draw_in_solar_mode",
            GridBasis::AcInput => "ac_input",
            GridBasis::LoadWithoutBattery => "load_without_battery",
            GridBasis::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GridStatus {
    pub on: Option<bool>,
    pub basis: GridBasis,
    pub voltage: Option<f64>,
    pub power_w: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GridInputs {
    pub mode: Option<u32>,
    pub battery_a: Option<f64>,
    pub battery_v: Option<f64>,
    pub ac_input_v: Option<f64>,
    pub load_w: Option<f64>,
    pub pv_w: Option<f64>,
}

/// Whether the battery is carrying the house: discharging over the limit and,
/// when load and voltage are known, covering most of what solar doesn't.
fn battery_carries_house(inputs: &GridInputs) -> bool {
    let Some(current) = inputs.battery_a.filter(|current| *current < -SOLAR_MODE_DISCHARGE_LIMIT_A) else {
        return false;
    };
    match (inputs.load_w, inputs.battery_v.filter(|v| *v > 0.0)) {
        (Some(load), Some(volts)) => {
            let shortfall = (load - inputs.pv_w.unwrap_or(0.0)).max(0.0);
            -current * volts >= shortfall * OUTAGE_BATTERY_SHARE
        }
        _ => true,
    }
}

/// In Solar output mode (solar, then grid, then battery) the battery only
/// carries the house once both solar and mains are gone, so that outranks a
/// possibly stale AC reading.
pub fn resolve_grid(inputs: GridInputs) -> (Option<bool>, GridBasis) {
    let solar_mode = inputs.mode == Some(SOLAR_OUTPUT_MODE);
    if solar_mode && battery_carries_house(&inputs) {
        return (Some(false), GridBasis::BatteryDrawInSolarMode);
    }
    if let Some(voltage) = inputs.ac_input_v {
        return (Some(voltage >= MAINS_PRESENT_VOLTS), GridBasis::AcInput);
    }
    if solar_mode {
        if let (Some(current), Some(load)) = (inputs.battery_a, inputs.load_w) {
            if current >= -SOLAR_MODE_DISCHARGE_LIMIT_A && load > inputs.pv_w.unwrap_or(0.0) + LOAD_OVER_PV_MARGIN_W {
                return (Some(true), GridBasis::LoadWithoutBattery);
            }
        }
    }
    (None, GridBasis::Unknown)
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
    pub energy_flow_stale: bool,
    pub grid: Option<GridStatus>,
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
        if unit.eq_ignore_ascii_case("kW") {
            Some(raw * 1000.0)
        } else if unit.is_empty() || unit.eq_ignore_ascii_case("W") {
            Some(raw)
        } else {
            None
        }
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

    pub fn output_mode(&self) -> Option<u32> {
        self.settings.as_ref().and_then(|settings| settings.output_source_priority_value)
    }

    pub fn grid_status(&self, bms_current_a: Option<f64>, bms_voltage: Option<f64>) -> GridStatus {
        let ac_input_v = self.field(&["acInputVoltage"]);
        let (on, basis) = resolve_grid(GridInputs {
            mode: self.output_mode(),
            battery_a: bms_current_a,
            battery_v: bms_voltage.or_else(|| self.field(&["batteryVoltage", "battery_voltage"])),
            ac_input_v,
            load_w: self.load_watts(),
            pv_w: self.pv_watts(),
        });
        GridStatus {
            on,
            basis,
            voltage: ac_input_v,
            power_w: Self::flow_watts(&self.grid_flow).or_else(|| self.field(&["gridPower"])),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flow(watts: f64, unit: &str) -> Value {
        json!({ "value": { "value": watts, "unit": unit } })
    }

    fn snapshot_with(soc: f64, discharge_a: f64, capacity_ah: f64, mode: u32) -> InverterSnapshot {
        snapshot_with_flows(soc, discharge_a, capacity_ah, mode, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot_with_flows(
        soc: f64,
        discharge_a: f64,
        capacity_ah: f64,
        mode: u32,
        pv_w: Option<f64>,
        load_w: Option<f64>,
        ac_input_voltage: Option<f64>,
    ) -> InverterSnapshot {
        let mut fields = Map::new();
        fields.insert("batterySOC".into(), json!({"value": soc.to_string()}));
        fields.insert(
            "batteryDischargeCurrent".into(),
            json!({"value": discharge_a.to_string()}),
        );
        fields.insert("batteryCapacity".into(), json!({"value": capacity_ah.to_string()}));
        if let Some(voltage) = ac_input_voltage {
            fields.insert("acInputVoltage".into(), json!({"value": voltage.to_string()}));
        }
        InverterSnapshot {
            device_id: "test".into(),
            fields,
            groups: vec![],
            firing_alarms: vec![],
            settings: Some(InverterSettings {
                output_source_priority: output_mode_name(mode),
                output_source_priority_value: Some(mode),
                charger_source_priority: "Solar + Utility".into(),
                charger_source_priority_value: Some(0),
                ac_input_range: None,
                battery_power_limiting: None,
                low_battery_cutoff_voltage: None,
                high_cutoff_voltage: None,
                low_dc_cutoff_soc: None,
                max_total_charge_current: None,
                max_utility_charge_current: None,
                smart_load: None,
            }),
            pv_panel_flow: pv_w.map(|watts| flow(watts, "W")),
            grid_flow: None,
            load_flow: load_w.map(|watts| flow(watts, "W")),
            energy_flow_stale: false,
            grid: None,
        }
    }

    #[test]
    fn snapshot_reads_latest_field_values() {
        let snapshot = snapshot_with(80.0, 20.0, 100.0, 0);
        assert_eq!(snapshot.field(&["batterySOC"]), Some(80.0));
        assert_eq!(snapshot.field(&["batteryDischargeCurrent"]), Some(20.0));
        assert_eq!(snapshot.field(&["batteryCapacity"]), Some(100.0));
    }

    #[test]
    fn flow_watts_reads_scalar_and_kilowatt_flows() {
        let snapshot = snapshot_with_flows(80.0, 0.0, 100.0, 0, Some(1500.0), Some(200.0), Some(230.0));
        assert_eq!(snapshot.pv_watts(), Some(1500.0));
        assert_eq!(snapshot.load_watts(), Some(200.0));
        assert_eq!(snapshot.grid_status(None, None).on, Some(true));

        let kilowatt = InverterSnapshot {
            pv_panel_flow: Some(flow(1.5, "kW")),
            ..snapshot.clone()
        };
        assert_eq!(kilowatt.pv_watts(), Some(1500.0));

        let grid_voltage = InverterSnapshot {
            grid_flow: Some(flow(230.1, "V")),
            ..snapshot.clone()
        };
        assert_eq!(grid_voltage.grid_status(None, None).power_w, None);
    }

    #[test]
    fn grid_on_flags_mains_below_100_volts() {
        let off = snapshot_with_flows(80.0, 0.0, 100.0, 1, None, None, Some(0.0)).grid_status(None, None);
        assert_eq!((off.on, off.basis), (Some(false), GridBasis::AcInput));
        let on = snapshot_with_flows(80.0, 0.0, 100.0, 1, None, None, Some(231.0)).grid_status(None, None);
        assert_eq!((on.on, on.voltage), (Some(true), Some(231.0)));
        assert_eq!(snapshot_with(80.0, 0.0, 100.0, 1).grid_status(None, None).on, None);
    }

    fn inputs(mode: u32, battery_a: Option<f64>, ac: Option<f64>, load: Option<f64>, pv: Option<f64>) -> GridInputs {
        GridInputs {
            mode: Some(mode),
            battery_a,
            battery_v: Some(26.0),
            ac_input_v: ac,
            load_w: load,
            pv_w: pv,
        }
    }

    #[test]
    fn battery_draw_in_solar_mode_means_grid_off_even_with_stale_voltage() {
        assert_eq!(
            resolve_grid(inputs(0, Some(-16.0), Some(230.0), Some(400.0), Some(0.0))),
            (Some(false), GridBasis::BatteryDrawInSolarMode)
        );
    }

    #[test]
    fn a_trickle_from_the_battery_while_the_grid_carries_the_house_is_not_an_outage() {
        assert_eq!(
            resolve_grid(inputs(0, Some(-1.2), Some(230.0), Some(101.0), Some(14.0))),
            (Some(true), GridBasis::AcInput)
        );
    }

    #[test]
    fn without_load_data_the_plain_discharge_rule_applies() {
        let bms_only = GridInputs { mode: Some(0), battery_a: Some(-3.0), ..GridInputs::default() };
        assert_eq!(resolve_grid(bms_only), (Some(false), GridBasis::BatteryDrawInSolarMode));
    }

    #[test]
    fn small_draw_in_solar_mode_is_not_an_outage() {
        assert_eq!(resolve_grid(inputs(0, Some(-0.5), Some(230.0), None, None)), (Some(true), GridBasis::AcInput));
    }

    #[test]
    fn battery_draw_in_sbg_mode_is_expected_not_an_outage() {
        assert_eq!(resolve_grid(inputs(1, Some(-12.0), None, Some(600.0), Some(0.0))), (None, GridBasis::Unknown));
    }

    #[test]
    fn load_covered_without_battery_in_solar_mode_means_grid_on() {
        assert_eq!(
            resolve_grid(inputs(0, Some(0.2), None, Some(500.0), Some(100.0))),
            (Some(true), GridBasis::LoadWithoutBattery)
        );
        assert_eq!(resolve_grid(inputs(0, Some(0.2), None, Some(150.0), Some(100.0))), (None, GridBasis::Unknown));
    }

    #[test]
    fn nothing_known_means_unknown() {
        assert_eq!(resolve_grid(GridInputs::default()), (None, GridBasis::Unknown));
    }
}