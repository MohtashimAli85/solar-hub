pub mod commands;
pub mod records;
pub mod runner;
pub mod units;

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify};

use crate::automation::history::HistoryStore;
use records::EnergyRecords;
use units::{bill_period, compute_day, split_day, BillPeriod, DayUnits, HourUnits, MeterAssignment, Reading, UnitPoint};

const RECENT_DAYS: i64 = 14;

const READING_TIME_FORMAT: &str = "%H:%M";

fn default_reading_day() -> u32 {
    23
}

fn default_reading_time() -> String {
    "11:57".into()
}

/// Fitted from the 23 Aug – 23 Sep bills (135 units against 131 counted over
/// 438 grid-on hours) and the first days after.
fn default_standby_w() -> f64 {
    11.0
}

pub const MAX_STANDBY_W: f64 = 100.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnergyConfig {
    #[serde(default = "default_reading_day")]
    pub bill_reading_day: u32,
    #[serde(default = "default_reading_time")]
    pub bill_reading_time: String,
    #[serde(default = "default_standby_w")]
    pub standby_w: f64,
}

impl Default for EnergyConfig {
    fn default() -> Self {
        Self {
            bill_reading_day: default_reading_day(),
            bill_reading_time: default_reading_time(),
            standby_w: default_standby_w(),
        }
    }
}

pub fn parse_reading_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), READING_TIME_FORMAT).ok()
}

impl EnergyConfig {
    pub fn reading(&self) -> Reading {
        Reading {
            day: self.bill_reading_day.clamp(1, 28),
            time: parse_reading_time(&self.bill_reading_time).unwrap_or(NaiveTime::MIN),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Backfill {
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnergySummary {
    pub configured: bool,
    pub today: Option<DayUnits>,
    pub today_hourly: Vec<HourUnits>,
    pub yesterday: Option<DayUnits>,
    pub recent_days: Vec<DayUnits>,
    pub bill: Option<BillPeriod>,
    pub previous_bill: Option<BillPeriod>,
    pub bill_reading_day: u32,
    pub bill_reading_time: String,
    pub standby_w: f64,
    pub active_meter: Option<u8>,
    pub active_since: Option<NaiveDateTime>,
    pub assignments: Vec<MeterAssignment>,
    pub updated_at: Option<NaiveDateTime>,
    pub error: Option<String>,
    pub backfill: Option<Backfill>,
    pub records_dir: String,
}

struct Live {
    tz: Tz,
    configured: bool,
    today: Option<NaiveDate>,
    today_points: Vec<UnitPoint>,
    updated_at: Option<NaiveDateTime>,
    error: Option<String>,
    backfill: Option<Backfill>,
    reading_days: BTreeMap<NaiveDate, Vec<UnitPoint>>,
}

#[derive(Clone)]
pub struct EnergyState {
    pub records: EnergyRecords,
    pub wake: Arc<Notify>,
    config: Arc<Mutex<EnergyConfig>>,
    live: Arc<Mutex<Live>>,
    summary: Arc<Mutex<Option<EnergySummary>>>,
    records_dir: String,
}

pub fn local_now(tz: Tz) -> NaiveDateTime {
    let now = Utc::now().with_timezone(&tz).naive_local();
    now.with_nanosecond(0).unwrap_or(now)
}

/// When a changeover recorded now took place: now, or an earlier time today
/// ("HH:MM"), never before the last recorded change.
pub fn switch_time(now: NaiveDateTime, last: Option<NaiveDateTime>, at: Option<&str>) -> Result<NaiveDateTime, String> {
    let when = match at.map(str::trim).filter(|text| !text.is_empty()) {
        None => now,
        Some(text) => {
            let time = NaiveTime::parse_from_str(text, "%H:%M").map_err(|_| "Use a time like 14:05.".to_string())?;
            let when = now.date().and_time(time);
            if when > now {
                return Err("That time hasn't come yet today.".into());
            }
            when
        }
    };
    match last {
        Some(last) if when < last => Err(format!("The last change was at {}; pick a later time.", last.format("%H:%M"))),
        _ => Ok(when),
    }
}

impl EnergyState {
    pub fn new(config: EnergyConfig, store: HistoryStore) -> Self {
        let records_dir = store.root().join("energy").display().to_string();
        Self {
            records: EnergyRecords::new(store),
            wake: Arc::new(Notify::new()),
            config: Arc::new(Mutex::new(config)),
            live: Arc::new(Mutex::new(Live {
                tz: chrono_tz::Asia::Karachi,
                configured: true,
                today: None,
                today_points: Vec::new(),
                updated_at: None,
                error: None,
                backfill: None,
                reading_days: BTreeMap::new(),
            })),
            summary: Arc::new(Mutex::new(None)),
            records_dir,
        }
    }

    pub async fn config(&self) -> EnergyConfig {
        self.config.lock().await.clone()
    }

    pub async fn set_config(&self, config: EnergyConfig) {
        *self.config.lock().await = config;
    }

    pub async fn now(&self) -> NaiveDateTime {
        local_now(self.live.lock().await.tz)
    }

    pub(crate) async fn set_tz(&self, tz: Tz, configured: bool) {
        let mut live = self.live.lock().await;
        live.tz = tz;
        live.configured = configured;
    }

    pub(crate) async fn set_today(&self, day: NaiveDate, points: Vec<UnitPoint>, at: NaiveDateTime) {
        let mut live = self.live.lock().await;
        live.today = Some(day);
        live.today_points = points;
        live.updated_at = Some(at);
    }

    pub(crate) async fn set_error(&self, error: Option<String>) {
        self.live.lock().await.error = error;
    }

    pub(crate) async fn has_reading_day(&self, day: NaiveDate) -> bool {
        self.live.lock().await.reading_days.contains_key(&day)
    }

    pub(crate) async fn set_reading_day_points(&self, day: NaiveDate, points: Vec<UnitPoint>) {
        self.live.lock().await.reading_days.insert(day, points);
    }

    pub(crate) async fn set_backfill(&self, backfill: Option<Backfill>) {
        self.live.lock().await.backfill = backfill;
    }

    pub async fn summary(&self) -> Option<EnergySummary> {
        self.summary.lock().await.clone()
    }

    /// Rebuilds the summary from the saved days plus today's cached points; no
    /// network, so meter and reading-day changes show up at once.
    pub async fn rebuild(&self) -> EnergySummary {
        let config = self.config().await;
        let reading = config.reading();
        let standby_w = config.standby_w.clamp(0.0, MAX_STANDBY_W);
        let switches = self.records.meter_switches();
        let assignments = self.records.meter_assignments();
        let first_switch = switches.first().map(|switch| switch.at);
        let live = self.live.lock().await;
        let now = local_now(live.tz);
        let today = now.date();
        let today_report = (live.today == Some(today)).then(|| compute_day(&live.today_points, &switches, today, now));
        let current_start = reading.period_start(now);
        let previous_start = reading.previous_period_start(now);
        let splits: BTreeMap<NaiveDate, units::DaySplit> = [previous_start, current_start]
            .into_iter()
            .filter_map(|start| {
                let points = if start.date() == today && live.today == Some(today) {
                    &live.today_points
                } else {
                    live.reading_days.get(&start.date())?
                };
                let day_end = (start.date().and_time(NaiveTime::MIN) + Duration::days(1)).min(now);
                let split = split_day(points, &switches, start, now)
                    .with_standby(standby_w)
                    .with_assignments(start, day_end, first_switch, &assignments);
                Some((start.date(), split))
            })
            .collect();
        let (configured, updated_at, error, backfill) = (live.configured, live.updated_at, live.error.clone(), live.backfill);
        drop(live);

        let active = switches.last().copied();
        let mut days: Vec<DayUnits> = self
            .records
            .days_between(previous_start.date(), today)
            .into_iter()
            .filter(|day| day.date < today)
            .map(|day| {
                let start = day.date.and_time(NaiveTime::MIN);
                day.with_standby(standby_w).with_assignments((start, start + Duration::days(1)), first_switch, &assignments)
            })
            .collect();
        let (today_units, today_hourly) = match today_report {
            Some((units, hourly)) => {
                let start = today.and_time(NaiveTime::MIN);
                (Some(units.with_standby(standby_w).with_assignments((start, now), first_switch, &assignments)), hourly)
            }
            None => (None, Vec::new()),
        };
        if let Some(units) = &today_units {
            days.push(units.clone());
        }
        let yesterday = today.pred_opt().and_then(|date| days.iter().find(|day| day.date == date).cloned());
        let recent_days = (0..RECENT_DAYS)
            .rev()
            .map(|back| today - Duration::days(back))
            .map(|date| days.iter().find(|day| day.date == date).cloned().unwrap_or_else(|| DayUnits::empty(date)))
            .collect();
        let active_meter = active.map(|switch| switch.meter);
        let summary = EnergySummary {
            configured,
            today: today_units,
            today_hourly,
            yesterday,
            recent_days,
            bill: Some(bill_period(&days, &splits, current_start, now, active_meter)),
            previous_bill: Some(bill_period(&days, &splits, previous_start, now, active_meter)),
            bill_reading_day: reading.day,
            bill_reading_time: reading.time.format(READING_TIME_FORMAT).to_string(),
            standby_w,
            active_meter,
            active_since: active.map(|switch| switch.at),
            assignments,
            updated_at,
            error,
            backfill,
            records_dir: self.records_dir.clone(),
        };
        *self.summary.lock().await = Some(summary.clone());
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn reading_config_defaults_and_parses() {
        let config = EnergyConfig::default();
        assert_eq!(config.reading(), Reading { day: 23, time: NaiveTime::from_hms_opt(11, 57, 0).unwrap() });
        let old: EnergyConfig = serde_json::from_str(r#"{"bill_reading_day": 12}"#).unwrap();
        assert_eq!(old.bill_reading_time, "11:57");
        assert_eq!(old.standby_w, 11.0);
        let odd = EnergyConfig { bill_reading_day: 31, bill_reading_time: "noon".into(), standby_w: 0.0 };
        assert_eq!(odd.reading(), Reading { day: 28, time: NaiveTime::MIN });
    }

    #[test]
    fn switch_defaults_to_now() {
        assert_eq!(switch_time(at(14, 5), None, None), Ok(at(14, 5)));
        assert_eq!(switch_time(at(14, 5), None, Some("  ")), Ok(at(14, 5)));
    }

    #[test]
    fn switch_can_be_backdated_within_today() {
        assert_eq!(switch_time(at(14, 5), Some(at(9, 0)), Some("13:30")), Ok(at(13, 30)));
    }

    #[test]
    fn switch_rejects_future_bad_and_out_of_order_times() {
        assert!(switch_time(at(14, 5), None, Some("15:00")).is_err());
        assert!(switch_time(at(14, 5), None, Some("2pm")).is_err());
        assert!(switch_time(at(14, 5), Some(at(13, 0)), Some("12:00")).is_err());
        let yesterday_evening = at(22, 0) - Duration::days(1);
        assert_eq!(switch_time(at(0, 30), Some(yesterday_evening), Some("00:10")), Ok(at(0, 10)));
    }
}
