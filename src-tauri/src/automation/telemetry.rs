use crate::automation::config::AutomationConfig;
use crate::battery::types::BatterySnapshot;
use crate::inverter::types::{output_mode_name, InverterSnapshot};

const INVERTER_EFFICIENCY: f64 = 0.9;

pub struct EngineReading {
    pub soc: Option<f64>,
    pub discharge_a: f64,
    pub charge_a: Option<f64>,
    pub usable_capacity_ah: f64,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub batt_v: Option<f64>,
    pub grid_on: Option<bool>,
    pub mode: Option<u32>,
    pub smart_load: Option<u32>,
    pub source: &'static str,
}

impl EngineReading {
    pub async fn from_snapshot(
        snapshot: &InverterSnapshot,
        bms: Option<&BatterySnapshot>,
        config: &AutomationConfig,
    ) -> Self {
        let mode = snapshot
            .settings
            .as_ref()
            .and_then(|settings| settings.output_source_priority_value);
        let smart_load = snapshot.settings.as_ref().and_then(|settings| settings.smart_load);
        let pv_w = snapshot.pv_watts();
        let load_w = snapshot.load_watts();
        let batt_v = snapshot.field(&["batteryVoltage", "battery_voltage"]);
        let grid_on = snapshot.grid_on();

        let Some(telemetry) = resolve_telemetry(bms, snapshot, config) else {
            return Self {
                soc: None,
                discharge_a: 0.0,
                charge_a: None,
                usable_capacity_ah: 0.0,
                pv_w,
                load_w,
                batt_v,
                grid_on,
                mode,
                smart_load,
                source: "inverter",
            };
        };

        // Battery current (charge and discharge) only ever comes from the BLE
        // BMS — see README.md, "Battery current source". There is no current
        // sensor between the battery and the inverter, so the inverter never
        // has a real reading to fall back to.
        let charge_a = bms.map(|bms| bms.current).filter(|current| *current > 0.0);

        Self {
            soc: Some(telemetry.soc),
            discharge_a: telemetry.discharge_a,
            charge_a,
            usable_capacity_ah: telemetry.usable_capacity_ah,
            pv_w,
            load_w,
            batt_v,
            grid_on,
            mode,
            smart_load,
            source: telemetry.source,
        }
    }

    pub fn mode_name(&self) -> Option<String> {
        self.mode.map(output_mode_name)
    }
}

pub struct BatteryTelemetry {
    pub soc: f64,
    pub discharge_a: f64,
    pub usable_capacity_ah: f64,
    pub source: &'static str,
}

pub fn resolve_telemetry(
    bms: Option<&BatterySnapshot>,
    snapshot: &InverterSnapshot,
    config: &AutomationConfig,
) -> Option<BatteryTelemetry> {
    if let Some(bms) = bms {
        let discharge_a = if bms.current < 0.0 { -bms.current } else { 0.0 };
        let usable_capacity_ah =
            (bms.remaining_capacity - bms.rated_capacity * config.min_soc_percent / 100.0).max(0.0);
        return Some(BatteryTelemetry {
            soc: bms.soc as f64,
            discharge_a,
            usable_capacity_ah,
            source: "bms",
        });
    }

    let soc = snapshot.field(&["batterySOC"])?;
    // No BMS connected means no real current reading — the inverter has no
    // current sensor of its own on the battery side of the wiring.
    let discharge_a = 0.0;
    let capacity_ah = snapshot.field(&["batteryCapacity"]).unwrap_or(config.capacity_ah);
    let usable_capacity_ah = ((soc - config.min_soc_percent) * capacity_ah / 100.0).max(0.0);
    Some(BatteryTelemetry {
        soc,
        discharge_a,
        usable_capacity_ah,
        source: "inverter",
    })
}

/// `None` runtime means "indefinite" (no discharge, capacity remains).
pub fn project_runtime(usable_capacity_ah: f64, discharge_a: f64) -> Option<f64> {
    if discharge_a > 0.0 {
        Some((usable_capacity_ah / discharge_a).max(0.0))
    } else if usable_capacity_ah > 0.0 {
        None
    } else {
        Some(0.0)
    }
}

pub fn estimate_discharge_a(load_w: Option<f64>, pv_w: Option<f64>, batt_v: Option<f64>) -> Option<f64> {
    let load = load_w?;
    let voltage = batt_v.filter(|voltage| *voltage > 0.0)?;
    let pv = pv_w.unwrap_or(0.0);
    Some(((load - pv).max(0.0)) / voltage / INVERTER_EFFICIENCY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bms_with(soc: u8, current: f64, remaining: f64, rated: f64) -> BatterySnapshot {
        let mut bms = BatterySnapshot::new("bms".into(), "test battery".into());
        bms.soc = soc;
        bms.current = current;
        bms.remaining_capacity = remaining;
        bms.rated_capacity = rated;
        bms
    }

    fn empty_snapshot() -> InverterSnapshot {
        InverterSnapshot {
            device_id: "test".into(),
            fields: serde_json::Map::new(),
            groups: vec![],
            firing_alarms: vec![],
            settings: None,
            pv_panel_flow: None,
            grid_flow: None,
            load_flow: None,
        }
    }

    #[test]
    fn resolver_prefers_connected_bms() {
        let config = AutomationConfig {
            min_soc_percent: 15.0,
            ..AutomationConfig::default()
        };
        let bms = bms_with(77, -18.5, 77.0, 100.0);
        let telemetry = resolve_telemetry(Some(&bms), &empty_snapshot(), &config).unwrap();
        assert_eq!(telemetry.source, "bms");
        assert_eq!(telemetry.soc, 77.0);
        assert!((telemetry.discharge_a - 18.5).abs() < 1e-9);
        assert!((telemetry.usable_capacity_ah - (77.0 - 15.0)).abs() < 1e-9);
    }

    #[test]
    fn resolver_returns_none_without_any_soc() {
        let config = AutomationConfig::default();
        assert!(resolve_telemetry(None, &empty_snapshot(), &config).is_none());
    }

    #[test]
    fn resolver_never_reads_current_from_the_inverter() {
        let mut fields = serde_json::Map::new();
        fields.insert("batterySOC".into(), serde_json::json!({"value": "80"}));
        fields.insert("batteryDischargeCurrent".into(), serde_json::json!({"value": "20"}));
        let snapshot = InverterSnapshot { fields, ..empty_snapshot() };
        let telemetry = resolve_telemetry(None, &snapshot, &AutomationConfig::default()).unwrap();
        assert_eq!(telemetry.source, "inverter");
        assert_eq!(telemetry.discharge_a, 0.0, "no current sensor between battery and inverter");
    }

    #[tokio::test]
    async fn charge_a_is_none_without_a_connected_bms() {
        let mut fields = serde_json::Map::new();
        fields.insert("batterySOC".into(), serde_json::json!({"value": "80"}));
        let snapshot = InverterSnapshot { fields, ..empty_snapshot() };
        let reading = EngineReading::from_snapshot(&snapshot, None, &AutomationConfig::default()).await;
        assert_eq!(reading.charge_a, None);
    }

    #[test]
    fn runtime_is_indefinite_without_discharge() {
        assert_eq!(project_runtime(65.0, 0.0), None);
    }

    #[test]
    fn runtime_is_zero_once_reserve_is_reached() {
        assert_eq!(project_runtime(0.0, 0.0), Some(0.0));
        assert_eq!(project_runtime(0.0, 20.0), Some(0.0));
    }

    #[test]
    fn estimate_math_derives_discharge_from_load() {
        assert!(
            (estimate_discharge_a(Some(2000.0), Some(100.0), Some(50.0)).unwrap() - 38.0 / INVERTER_EFFICIENCY)
                .abs()
                < 1e-9
        );
        assert_eq!(estimate_discharge_a(Some(2000.0), Some(5000.0), Some(50.0)), Some(0.0));
        assert_eq!(estimate_discharge_a(Some(100.0), Some(0.0), None), None);
        assert_eq!(estimate_discharge_a(None, Some(0.0), Some(50.0)), None);
    }
}
