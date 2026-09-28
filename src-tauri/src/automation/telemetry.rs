use chrono::{Duration, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::automation::config::AutomationConfig;
use crate::battery::types::BatterySnapshot;
use crate::inverter::types::{output_mode_name, resolve_grid, GridBasis, GridInputs, GridStatus, InverterSnapshot};

const INVERTER_EFFICIENCY: f64 = 0.9;
const PERFORMANCE_RATIO: f64 = 0.75;
const MIN_REFERENCE_RADIATION: f64 = 50.0;
const MAX_RADIATION_RATIO: f64 = 2.5;
pub const FULL_SOC: f64 = 99.5;

pub struct EngineReading {
    pub soc: Option<f64>,
    pub discharge_a: f64,
    pub charge_a: Option<f64>,
    /// Signed BMS current: positive charging, negative discharging.
    pub battery_a: Option<f64>,
    pub usable_capacity_ah: f64,
    pub rated_capacity_ah: f64,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub batt_v: Option<f64>,
    pub grid: GridStatus,
    pub mode: Option<u32>,
    pub smart_load: Option<bool>,
    /// The inverter's max total charge current setting.
    pub max_charge_a: Option<f64>,
    pub source: &'static str,
}

impl EngineReading {
    pub fn from_snapshot(snapshot: &InverterSnapshot, bms: Option<&BatterySnapshot>, config: &AutomationConfig) -> Self {
        let mode = snapshot.output_mode();
        let smart_load = snapshot.settings.as_ref().and_then(|s| s.smart_load).map(|value| value == 1);
        let max_charge_a = snapshot
            .settings
            .as_ref()
            .and_then(|s| s.max_total_charge_current)
            .filter(|amps| *amps > 0.0);
        let pv_w = snapshot.pv_watts();
        let load_w = snapshot.load_watts();
        let batt_v = bms
            .map(|bms| bms.voltage)
            .filter(|voltage| *voltage > 0.0)
            .or_else(|| snapshot.field(&["batteryVoltage", "battery_voltage"]));
        // Battery current (charge and discharge) only ever comes from the BLE
        // BMS — see README.md, "Battery current source". There is no current
        // sensor between the battery and the inverter.
        let battery_a = bms.map(|bms| bms.current);
        let grid = snapshot.grid_status(battery_a, bms.map(|bms| bms.voltage));
        let rated_capacity_ah = bms
            .map(|bms| bms.rated_capacity)
            .filter(|rated| *rated > 0.0)
            .or_else(|| snapshot.field(&["batteryCapacity"]))
            .unwrap_or(config.capacity_ah);

        let Some(telemetry) = resolve_telemetry(bms, snapshot, config) else {
            return Self {
                soc: None,
                discharge_a: 0.0,
                charge_a: None,
                battery_a,
                usable_capacity_ah: 0.0,
                rated_capacity_ah,
                pv_w,
                load_w,
                batt_v,
                grid,
                mode,
                smart_load,
                max_charge_a,
                source: "inverter",
            };
        };

        Self {
            soc: Some(telemetry.soc),
            discharge_a: telemetry.discharge_a,
            charge_a: battery_a.filter(|current| *current > 0.0),
            battery_a,
            usable_capacity_ah: telemetry.usable_capacity_ah,
            rated_capacity_ah,
            pv_w,
            load_w,
            batt_v,
            grid,
            mode,
            smart_load,
            max_charge_a,
            source: telemetry.source,
        }
    }

    /// Used when the inverter cloud is unreachable (often the internet goes
    /// down with the grid): the BLE BMS plus the last known mode still tell
    /// us whether the house is running off the battery.
    pub fn from_bms_only(bms: &BatterySnapshot, last_mode: Option<u32>, config: &AutomationConfig) -> Self {
        let (on, basis) = resolve_grid(GridInputs {
            mode: last_mode,
            battery_a: Some(bms.current),
            battery_v: Some(bms.voltage),
            ..GridInputs::default()
        });
        let rated_capacity_ah = if bms.rated_capacity > 0.0 { bms.rated_capacity } else { config.capacity_ah };
        let usable_capacity_ah =
            (bms.remaining_capacity - rated_capacity_ah * config.min_soc_percent / 100.0).max(0.0);
        Self {
            soc: Some(bms.soc as f64),
            discharge_a: (-bms.current).max(0.0),
            charge_a: Some(bms.current).filter(|current| *current > 0.0),
            battery_a: Some(bms.current),
            usable_capacity_ah,
            rated_capacity_ah,
            pv_w: None,
            load_w: None,
            batt_v: Some(bms.voltage).filter(|voltage| *voltage > 0.0),
            grid: GridStatus { on, basis, voltage: None, power_w: None },
            mode: last_mode,
            smart_load: None,
            max_charge_a: None,
            source: "bms_only",
        }
    }

    pub fn grid_on(&self) -> Option<bool> {
        self.grid.on
    }

    pub fn grid_basis(&self) -> GridBasis {
        self.grid.basis
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

pub fn estimate_discharge_a(load_w: Option<f64>, pv_w: Option<f64>, batt_v: Option<f64>) -> Option<f64> {
    let load = load_w?;
    let voltage = batt_v.filter(|voltage| *voltage > 0.0)?;
    let pv = pv_w.unwrap_or(0.0);
    Some(((load - pv).max(0.0)) / voltage / INVERTER_EFFICIENCY)
}

/// SOC percentage points one amp-hour is worth on this pack.
pub fn soc_per_ah(rated_ah: f64) -> f64 {
    if rated_ah > 0.0 { 100.0 / rated_ah } else { 0.0 }
}

/// Percent of SOC one hour at `load_w` costs when the house runs on battery.
pub fn soc_per_hour_at(load_w: f64, rated_ah: f64, batt_v: f64) -> Option<f64> {
    (rated_ah > 0.0 && batt_v > 0.0).then(|| load_w / batt_v / INVERTER_EFFICIENCY * soc_per_ah(rated_ah))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SocPoint {
    pub at: NaiveDateTime,
    pub soc: f64,
}

fn next_hour(at: NaiveDateTime) -> NaiveDateTime {
    at.date().and_hms_opt(at.hour(), 0, 0).unwrap_or(at) + Duration::hours(1)
}

/// Hour-by-hour SOC if the house runs on battery from `start` to `end`,
/// drawing `load_for_hour(hour)` watts in each clock hour.
pub fn soc_trajectory(
    start: NaiveDateTime,
    end: NaiveDateTime,
    soc: f64,
    rated_ah: f64,
    batt_v: f64,
    load_for_hour: impl Fn(u32) -> f64,
) -> Vec<SocPoint> {
    if rated_ah <= 0.0 || batt_v <= 0.0 || end <= start {
        return Vec::new();
    }
    let mut points = vec![SocPoint { at: start, soc }];
    let mut at = start;
    let mut level = soc;
    while at < end {
        let step_end = next_hour(at).min(end);
        let hours = (step_end - at).num_seconds() as f64 / 3600.0;
        let amps = load_for_hour(at.hour()).max(0.0) / batt_v / INVERTER_EFFICIENCY;
        level = (level - amps * hours * soc_per_ah(rated_ah)).max(0.0);
        points.push(SocPoint { at: step_end, soc: level });
        at = step_end;
    }
    points
}

pub fn first_at_or_below(points: &[SocPoint], soc: f64) -> Option<NaiveDateTime> {
    points.iter().find(|point| point.soc <= soc).map(|point| point.at)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMethod {
    PvArray,
    PvHeadroom,
    ChargeScaling,
    ConstantCharge,
    AfterSunset,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayProjection {
    pub soc_now: f64,
    pub soc_at_sunset: f64,
    pub full_at: Option<NaiveDateTime>,
    pub hours_of_sun_left: f64,
    pub ah_to_full: f64,
    pub charge_a: f64,
    pub max_charge_a: Option<f64>,
    pub method: ProjectionMethod,
}

impl DayProjection {
    pub fn fills(&self) -> bool {
        self.soc_at_sunset >= FULL_SOC
    }
}

pub struct DayProjectionInput<'a> {
    pub now: NaiveDateTime,
    pub sunset: NaiveDateTime,
    pub soc: f64,
    pub rated_ah: f64,
    pub batt_v: Option<f64>,
    pub charge_a: f64,
    pub max_charge_a: Option<f64>,
    pub pv_now_w: Option<f64>,
    pub pv_array_watts: f64,
    pub current_load_w: Option<f64>,
    pub radiation_during: &'a dyn Fn(NaiveDateTime) -> Option<f64>,
    pub typical_load_for_hour: &'a dyn Fn(u32) -> Option<f64>,
}

/// Where the battery ends up by sunset if charging follows the forecast sun.
/// It projects the charge the battery *could* take — forecast PV minus the
/// typical house load, capped at the inverter's max charge current — so a
/// temporary big load (an EV charging) doesn't read as "won't fill".
pub fn project_day(input: DayProjectionInput<'_>) -> DayProjection {
    let hours_of_sun_left = ((input.sunset - input.now).num_seconds() as f64 / 3600.0).max(0.0);
    let ah_to_full = ((100.0 - input.soc).max(0.0) / 100.0) * input.rated_ah;
    let base = DayProjection {
        soc_now: input.soc,
        soc_at_sunset: input.soc,
        full_at: (input.soc >= FULL_SOC).then_some(input.now),
        hours_of_sun_left,
        ah_to_full,
        charge_a: input.charge_a,
        max_charge_a: input.max_charge_a,
        method: ProjectionMethod::AfterSunset,
    };
    if input.now >= input.sunset || input.rated_ah <= 0.0 {
        return base;
    }

    let radiation_now = (input.radiation_during)(input.now);
    let voltage = input.batt_v.filter(|v| *v > 0.0);
    let pv_now = input.pv_now_w.filter(|pv| *pv > MIN_REFERENCE_RADIATION);
    let method = match (radiation_now, voltage, pv_now) {
        (Some(_), Some(_), _) if input.pv_array_watts > 0.0 => ProjectionMethod::PvArray,
        (Some(_), Some(_), Some(_)) => ProjectionMethod::PvHeadroom,
        (Some(_), _, _) => ProjectionMethod::ChargeScaling,
        _ => ProjectionMethod::ConstantCharge,
    };
    let reference = radiation_now.unwrap_or(0.0).max(MIN_REFERENCE_RADIATION);
    let house_load = |hour: u32| (input.typical_load_for_hour)(hour).or(input.current_load_w).unwrap_or(0.0);

    let mut level = input.soc;
    let mut full_at = base.full_at;
    let mut at = input.now;
    while at < input.sunset {
        let step_end = next_hour(at).min(input.sunset);
        let hours = (step_end - at).num_seconds() as f64 / 3600.0;
        let radiation = (input.radiation_during)(at);
        let amps = match (method, radiation, voltage) {
            (ProjectionMethod::PvArray, Some(rad), Some(v)) => {
                let pv = rad * input.pv_array_watts / 1000.0 * PERFORMANCE_RATIO;
                (pv - house_load(at.hour())).max(0.0) / v
            }
            (ProjectionMethod::PvHeadroom, Some(rad), Some(v)) => {
                let ratio = (rad / reference).clamp(0.0, MAX_RADIATION_RATIO);
                let pv = pv_now.unwrap_or(0.0) * ratio;
                let headroom = (pv - house_load(at.hour())).max(0.0) / v;
                headroom.max(input.charge_a * ratio)
            }
            (ProjectionMethod::ChargeScaling, Some(rad), _) => {
                input.charge_a * (rad / reference).clamp(0.0, MAX_RADIATION_RATIO)
            }
            _ => input.charge_a,
        };
        let amps = input.max_charge_a.map_or(amps, |max| amps.min(max));
        let rate = amps * soc_per_ah(input.rated_ah);
        let next_level = level + rate * hours;
        if full_at.is_none() && next_level >= 100.0 && rate > 0.0 {
            let hours_to_full = (100.0 - level) / rate;
            full_at = Some(at + Duration::seconds((hours_to_full * 3600.0) as i64));
        }
        level = next_level.min(100.0);
        at = step_end;
    }

    DayProjection {
        soc_at_sunset: level,
        full_at,
        method,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 26).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn bms_with(soc: u8, current: f64, remaining: f64, rated: f64) -> BatterySnapshot {
        let mut bms = BatterySnapshot::new("bms".into(), "test battery".into());
        bms.soc = soc;
        bms.current = current;
        bms.remaining_capacity = remaining;
        bms.rated_capacity = rated;
        bms.voltage = 51.2;
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
            energy_flow_stale: false,
            grid: None,
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

    #[test]
    fn charge_a_is_none_without_a_connected_bms() {
        let mut fields = serde_json::Map::new();
        fields.insert("batterySOC".into(), serde_json::json!({"value": "80"}));
        let snapshot = InverterSnapshot { fields, ..empty_snapshot() };
        let reading = EngineReading::from_snapshot(&snapshot, None, &AutomationConfig::default());
        assert_eq!(reading.charge_a, None);
        assert_eq!(reading.rated_capacity_ah, 100.0);
    }

    #[test]
    fn bms_only_reading_infers_outage_from_battery_draw_in_solar_mode() {
        let bms = bms_with(60, -6.0, 60.0, 100.0);
        let reading = EngineReading::from_bms_only(&bms, Some(0), &AutomationConfig::default());
        assert_eq!(reading.grid_on(), Some(false));
        assert_eq!(reading.grid_basis(), GridBasis::BatteryDrawInSolarMode);
        assert_eq!(reading.source, "bms_only");
    }

    #[test]
    fn estimate_math_derives_discharge_from_load() {
        assert!(
            (estimate_discharge_a(Some(2000.0), Some(100.0), Some(50.0)).unwrap() - 38.0 / INVERTER_EFFICIENCY).abs()
                < 1e-9
        );
        assert_eq!(estimate_discharge_a(Some(2000.0), Some(5000.0), Some(50.0)), Some(0.0));
        assert_eq!(estimate_discharge_a(Some(100.0), Some(0.0), None), None);
        assert_eq!(estimate_discharge_a(None, Some(0.0), Some(50.0)), None);
    }

    #[test]
    fn trajectory_follows_the_hourly_profile() {
        let load = |hour: u32| if (18..23).contains(&hour) { 900.0 } else { 225.0 };
        let points = soc_trajectory(at(21, 30), at(23, 30), 70.0, 100.0, 50.0, load);
        assert_eq!(points.len(), 4);
        assert_eq!(points[1].at, at(22, 0));
        let evening_rate = 900.0 / 50.0 / INVERTER_EFFICIENCY;
        let night_rate = 225.0 / 50.0 / INVERTER_EFFICIENCY;
        let expected = 70.0 - evening_rate * 1.5 - night_rate * 0.5;
        assert!((points[3].soc - expected).abs() < 1e-9);
        assert_eq!(first_at_or_below(&points, 50.0), Some(at(23, 0)));
    }

    #[test]
    fn trajectory_never_goes_below_zero() {
        let points = soc_trajectory(at(21, 0), at(23, 0), 5.0, 100.0, 50.0, |_| 5000.0);
        assert_eq!(points.last().unwrap().soc, 0.0);
    }

    fn day_input<'a>(
        now: NaiveDateTime,
        soc: f64,
        charge_a: f64,
        radiation: &'a dyn Fn(NaiveDateTime) -> Option<f64>,
        typical: &'a dyn Fn(u32) -> Option<f64>,
    ) -> DayProjectionInput<'a> {
        DayProjectionInput {
            now,
            sunset: at(18, 0),
            soc,
            rated_ah: 100.0,
            batt_v: Some(51.2),
            charge_a,
            max_charge_a: Some(45.0),
            pv_now_w: None,
            pv_array_watts: 0.0,
            current_load_w: Some(400.0),
            radiation_during: radiation,
            typical_load_for_hour: typical,
        }
    }

    #[test]
    fn ten_amps_on_a_sunny_morning_still_fills_by_sunset() {
        let sunny = |_: NaiveDateTime| Some(600.0);
        let none = |_: u32| None;
        let projection = project_day(day_input(at(11, 0), 60.0, 10.0, &sunny, &none));
        assert_eq!(projection.method, ProjectionMethod::ChargeScaling);
        assert!(projection.fills());
        assert_eq!(projection.full_at, Some(at(15, 0)));
    }

    #[test]
    fn cloudy_day_does_not_fill() {
        let cloudy = |_: NaiveDateTime| Some(100.0);
        let none = |_: u32| None;
        let projection = project_day(day_input(at(11, 0), 60.0, 2.0, &cloudy, &none));
        assert!(!projection.fills());
        assert!((projection.soc_at_sunset - 74.0).abs() < 1e-9);
    }

    #[test]
    fn nothing_charges_after_sunset() {
        let sunny = |_: NaiveDateTime| Some(600.0);
        let none = |_: u32| None;
        let projection = project_day(day_input(at(18, 30), 80.0, 10.0, &sunny, &none));
        assert_eq!(projection.method, ProjectionMethod::AfterSunset);
        assert_eq!(projection.soc_at_sunset, 80.0);
        assert_eq!(projection.hours_of_sun_left, 0.0);
    }

    #[test]
    fn ev_soaking_up_the_sun_still_fills_once_it_stops() {
        let sunny = |_: NaiveDateTime| Some(600.0);
        let typical = |_: u32| Some(400.0);
        let mut input = day_input(at(14, 0), 50.0, 10.0, &sunny, &typical);
        input.pv_now_w = Some(3000.0);
        input.current_load_w = Some(2500.0);
        let projection = project_day(input);
        assert_eq!(projection.method, ProjectionMethod::PvHeadroom);
        assert!(projection.fills(), "headroom is capped at 45 A, still fills in ~1.1 h");

        let no_sun_data = |_: NaiveDateTime| Some(600.0);
        let none = |_: u32| None;
        let only_measured = project_day(day_input(at(14, 0), 50.0, 10.0, &no_sun_data, &none));
        assert!(!only_measured.fills(), "10 A for 4 h without headroom info reaches only 90%");
    }

    #[test]
    fn charge_never_exceeds_the_inverter_limit() {
        let sunny = |_: NaiveDateTime| Some(900.0);
        let typical = |_: u32| Some(0.0);
        let mut input = day_input(at(17, 0), 10.0, 5.0, &sunny, &typical);
        input.pv_now_w = Some(9000.0);
        let projection = project_day(input);
        assert!((projection.soc_at_sunset - 55.0).abs() < 1e-9, "one hour at the 45 A cap = +45%");
    }

    #[test]
    fn array_size_projection_subtracts_typical_load() {
        let sunny = |_: NaiveDateTime| Some(800.0);
        let typical = |_: u32| Some(600.0);
        let mut input = day_input(at(17, 0), 50.0, 0.0, &sunny, &typical);
        input.pv_array_watts = 3000.0;
        let projection = project_day(input);
        assert_eq!(projection.method, ProjectionMethod::PvArray);
        let amps = (800.0 * 3.0 * PERFORMANCE_RATIO - 600.0) / 51.2;
        assert!((projection.soc_at_sunset - (50.0 + amps)).abs() < 1e-9);
    }
}
