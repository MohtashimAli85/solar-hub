use chrono::NaiveDateTime;
use serde::Serialize;

use crate::automation::history::{HourLoad, Outage};
use crate::automation::telemetry::{DayProjection, SocPoint};
use crate::automation::weather::WeatherSummary;
use crate::inverter::types::GridStatus;

#[derive(Debug, Clone, Default, Serialize)]
pub struct RoutineInsight {
    pub nights_with_data: u32,
    pub typical: Vec<HourLoad>,
    pub tonight: Vec<HourLoad>,
    pub quiet_by: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RecordsInfo {
    pub dir: String,
    pub samples_this_month: usize,
    pub decisions_this_month: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SmartLoadInsight {
    pub on: Option<bool>,
    /// At night it goes on once the battery drain holds at or above
    /// `trigger_amps` for `trigger_minutes`. 0 amps = never.
    pub trigger_amps: f64,
    pub trigger_minutes: u32,
    /// The latest battery drain the night has seen.
    pub drain_a: Option<f64>,
    /// The state the engine last set this window (true = enabled, heavy loads cut).
    pub applied: Option<bool>,
    /// Near morning with enough battery: disabled for the rest of the night.
    pub released: bool,
}

/// What the UI shows is built from the same inputs the agent sees.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationInsights {
    pub updated_at: Option<NaiveDateTime>,
    pub window: Option<&'static str>,
    pub sunrise: Option<NaiveDateTime>,
    pub sunset: Option<NaiveDateTime>,
    pub night_start: Option<NaiveDateTime>,
    pub next_sunrise: Option<NaiveDateTime>,
    pub soc: Option<f64>,
    pub simulated_soc: Option<f64>,
    pub floor_soc: f64,
    pub reserve_soc: Option<f64>,
    pub trajectory: Vec<SocPoint>,
    pub trajectory_is_preview: bool,
    pub trajectory_basis: Option<&'static str>,
    pub actual_soc: Vec<SocPoint>,
    pub reserve_eta: Option<NaiveDateTime>,
    pub backup_hours: Option<f64>,
    pub routine: RoutineInsight,
    pub weather: Option<WeatherSummary>,
    pub outages: Vec<Outage>,
    pub day: Option<DayProjection>,
    pub records: RecordsInfo,
    pub grid: Option<GridStatus>,
    pub smart_load: Option<SmartLoadInsight>,
}
