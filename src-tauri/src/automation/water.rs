use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::automation::config::AutomationConfig;
use crate::automation::telemetry::FULL_SOC;
use crate::automation::weather::Forecast;

pub const TITLE: &str = "Water tank";
const WATCH_MINUTES: i64 = 5;
/// The last-chance note is only sent this soon after the window closes, so
/// opening the app in the evening doesn't send a stale one.
const LAST_CALL_GRACE_MINUTES: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaterPhase {
    #[default]
    Off,
    Waiting,
    Ready,
    Done,
    Missed,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct WaterStatus {
    pub phase: WaterPhase,
    pub note: Option<String>,
    pub spare_w: Option<f64>,
    pub last_reminded_at: Option<NaiveDateTime>,
}

#[derive(Debug, Default)]
pub struct WaterMemory {
    day: Option<NaiveDate>,
    last_reminded_at: Option<NaiveDateTime>,
    last_call_sent: bool,
}

pub struct WaterInput<'a> {
    pub now: NaiveDateTime,
    pub soc: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub forecast: Option<&'a Forecast>,
    pub pv_ratio: f64,
    pub config: &'a AutomationConfig,
    pub done_on: Option<NaiveDate>,
}

#[derive(Debug, PartialEq)]
pub struct WaterStep {
    pub status: WaterStatus,
    pub notify: Option<String>,
    pub next_at: Option<NaiveDateTime>,
}

enum Verdict {
    Ready { spare_w: Option<f64>, message: String },
    Waiting { spare_w: Option<f64>, note: String },
}

fn at_hour(date: NaiveDate, hour: u32) -> NaiveDateTime {
    date.and_hms_opt(hour.min(23), 0, 0).unwrap_or_else(|| date.and_time(chrono::NaiveTime::MIN))
}

fn hour_text(hour: u32) -> String {
    match hour {
        0 => "12 AM".into(),
        12 => "12 PM".into(),
        h if h < 12 => format!("{h} AM"),
        h => format!("{} PM", h - 12),
    }
}

fn forecast_pv_w(forecast: Option<&Forecast>, config: &AutomationConfig, pv_ratio: f64, at: NaiveDateTime) -> Option<f64> {
    if config.pv_array_watts <= 0.0 {
        return None;
    }
    forecast?
        .radiation_during(at)
        .map(|radiation| radiation * config.pv_array_watts / 1000.0 * pv_ratio)
}

fn best_hour_left(forecast: Option<&Forecast>, now: NaiveDateTime, end_hour: u32) -> bool {
    let Some(forecast) = forecast else {
        return true;
    };
    let Some(current) = forecast.radiation_during(now) else {
        return true;
    };
    (now.hour() + 1..end_hour)
        .filter_map(|hour| forecast.radiation_during(at_hour(now.date(), hour)))
        .all(|later| current >= later)
}

fn judge(input: &WaterInput) -> Verdict {
    let config = input.config;
    let motor = config.water_motor_watts;
    let Some(soc) = input.soc else {
        return Verdict::Waiting { spare_w: None, note: "Waiting for the battery level.".into() };
    };
    let soc_text = format!("{soc:.0}%");
    let full = soc >= config.water_full_soc.min(FULL_SOC);
    let live_spare = input.pv_w.zip(input.load_w).map(|(pv, load)| pv - load);
    let forecast_spare = forecast_pv_w(input.forecast, config, input.pv_ratio, input.now).map(|pv| pv - input.load_w.unwrap_or(0.0));

    if full {
        if let Some(spare) = forecast_spare.filter(|spare| *spare >= motor) {
            return Verdict::Ready {
                spare_w: Some(spare),
                message: format!(
                    "Turn on the motor. The battery is full and the sun can carry about {:.0} W more. Tap Done once the tank is full.",
                    spare
                ),
            };
        }
    }
    if let Some(spare) = live_spare.filter(|spare| *spare >= motor) {
        return Verdict::Ready {
            spare_w: Some(spare),
            message: format!(
                "Turn on the motor. Solar is making {:.0} W more than the house uses (battery {soc_text}). Tap Done once the tank is full.",
                spare
            ),
        };
    }

    let cloudy = !full && input.now.hour() >= config.water_cloudy_hour;
    if cloudy {
        if soc < config.water_cloudy_soc {
            return Verdict::Waiting {
                spare_w: live_spare,
                note: format!(
                    "Weak sun today: battery is {soc_text}, below the {:.0}% needed.",
                    config.water_cloudy_soc
                ),
            };
        }
        if best_hour_left(input.forecast, input.now, config.water_end_hour) {
            return Verdict::Ready {
                spare_w: live_spare,
                message: format!(
                    "Weak sun today and this is the best hour left. Turn on the motor now (battery {soc_text}). Tap Done once the tank is full."
                ),
            };
        }
        return Verdict::Waiting {
            spare_w: live_spare,
            note: "Weak sun today: waiting for the sunniest hour left.".into(),
        };
    }

    let note = if full {
        match forecast_spare {
            None if config.pv_array_watts <= 0.0 => {
                "Battery is full. Set the PV array size so the forecast can tell how much solar is spare.".to_string()
            }
            None => "Battery is full, but there's no forecast for this hour yet.".to_string(),
            Some(spare) => format!("Battery is full, but the forecast shows only about {spare:.0} W spare."),
        }
    } else {
        let live = live_spare.map(|spare| format!(" (now {:.0} W)", spare.max(0.0))).unwrap_or_default();
        format!(
            "Battery {soc_text}: waiting for {:.0}% or {:.0} W spare solar{live}.",
            config.water_full_soc, motor
        )
    };
    Verdict::Waiting { spare_w: forecast_spare.or(live_spare), note }
}

pub fn step(memory: &mut WaterMemory, input: &WaterInput) -> WaterStep {
    let config = input.config;
    let now = input.now;
    let today = now.date();
    if memory.day != Some(today) {
        *memory = WaterMemory { day: Some(today), ..WaterMemory::default() };
    }
    let status = |phase, note: Option<String>, spare_w| WaterStatus {
        phase,
        note,
        spare_w,
        last_reminded_at: memory.last_reminded_at,
    };

    if !config.water_reminder_enabled {
        return WaterStep { status: status(WaterPhase::Off, None, None), notify: None, next_at: None };
    }
    if input.done_on == Some(today) {
        return WaterStep { status: status(WaterPhase::Done, None, None), notify: None, next_at: None };
    }

    let start = at_hour(today, config.water_start_hour);
    let end = at_hour(today, config.water_end_hour);
    if now < start {
        let note = format!("Reminders start at {}.", hour_text(config.water_start_hour));
        return WaterStep { status: status(WaterPhase::Waiting, Some(note), None), notify: None, next_at: Some(start) };
    }
    if now >= end {
        let note = Some("Tank not filled today.".to_string());
        let notify = (!memory.last_call_sent && now < end + Duration::minutes(LAST_CALL_GRACE_MINUTES)).then(|| {
            format!("Tank not filled today. It's past {} and it hasn't been marked done.", hour_text(config.water_end_hour))
        });
        memory.last_call_sent = true;
        return WaterStep { status: status(WaterPhase::Missed, note, None), notify, next_at: None };
    }

    let watch = (now + Duration::minutes(WATCH_MINUTES)).min(end);
    match judge(input) {
        Verdict::Waiting { spare_w, note } => {
            WaterStep { status: status(WaterPhase::Waiting, Some(note), spare_w), notify: None, next_at: Some(watch) }
        }
        Verdict::Ready { spare_w, message } => {
            let repeat = Duration::minutes(i64::from(config.water_repeat_minutes));
            let due = memory.last_reminded_at.is_none_or(|last| now - last >= repeat);
            if due {
                memory.last_reminded_at = Some(now);
            }
            let next_at = memory.last_reminded_at.map(|last| (last + repeat).min(end));
            WaterStep {
                status: WaterStatus {
                    phase: WaterPhase::Ready,
                    note: Some(message.clone()),
                    spare_w,
                    last_reminded_at: memory.last_reminded_at,
                },
                notify: due.then_some(message),
                next_at,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::weather::HourWeather;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn config() -> AutomationConfig {
        AutomationConfig { pv_array_watts: 4000.0, ..AutomationConfig::default() }
    }

    fn forecast(radiation: &[(u32, f64)]) -> Forecast {
        Forecast {
            hourly: radiation
                .iter()
                .map(|(hour, w)| HourWeather { at: at(*hour, 0), temp_c: None, cloud_pct: None, radiation_w_m2: Some(*w) })
                .collect(),
            daily: Vec::new(),
        }
    }

    fn input<'a>(now: NaiveDateTime, soc: f64, pv: f64, load: f64, forecast: Option<&'a Forecast>, config: &'a AutomationConfig) -> WaterInput<'a> {
        WaterInput { now, soc: Some(soc), pv_w: Some(pv), load_w: Some(load), forecast, pv_ratio: 0.75, config, done_on: None }
    }

    #[test]
    fn full_battery_uses_forecast_since_live_pv_is_held_down() {
        let config = config();
        let sun = forecast(&[(12, 700.0)]);
        let mut memory = WaterMemory::default();
        let result = step(&mut memory, &input(at(11, 30), 100.0, 300.0, 300.0, Some(&sun), &config));
        assert_eq!(result.status.phase, WaterPhase::Ready);
        assert!(result.notify.is_some());
        assert_eq!(result.next_at, Some(at(11, 45)));
    }

    #[test]
    fn full_battery_without_panel_size_waits() {
        let config = AutomationConfig::default();
        let sun = forecast(&[(12, 700.0)]);
        let mut memory = WaterMemory::default();
        let result = step(&mut memory, &input(at(11, 30), 100.0, 300.0, 300.0, Some(&sun), &config));
        assert_eq!(result.status.phase, WaterPhase::Waiting);
        assert!(result.notify.is_none());
    }

    #[test]
    fn charging_battery_reminds_on_live_spare() {
        let config = config();
        let mut memory = WaterMemory::default();
        let result = step(&mut memory, &input(at(12, 0), 70.0, 2500.0, 400.0, None, &config));
        assert_eq!(result.status.phase, WaterPhase::Ready);
    }

    #[test]
    fn repeats_only_after_the_interval() {
        let config = config();
        let mut memory = WaterMemory::default();
        assert!(step(&mut memory, &input(at(12, 0), 70.0, 2500.0, 400.0, None, &config)).notify.is_some());
        assert!(step(&mut memory, &input(at(12, 10), 70.0, 2500.0, 400.0, None, &config)).notify.is_none());
        assert!(step(&mut memory, &input(at(12, 15), 70.0, 2500.0, 400.0, None, &config)).notify.is_some());
    }

    #[test]
    fn done_stops_reminders_for_the_day() {
        let config = config();
        let mut memory = WaterMemory::default();
        let mut done = input(at(12, 0), 70.0, 2500.0, 400.0, None, &config);
        done.done_on = Some(at(12, 0).date());
        let result = step(&mut memory, &done);
        assert_eq!(result.status.phase, WaterPhase::Done);
        assert!(result.notify.is_none());
    }

    #[test]
    fn nothing_outside_the_window() {
        let config = config();
        let mut memory = WaterMemory::default();
        let result = step(&mut memory, &input(at(10, 0), 100.0, 3000.0, 300.0, None, &config));
        assert_eq!(result.status.phase, WaterPhase::Waiting);
        assert!(result.notify.is_none());
        assert_eq!(result.next_at, Some(at(11, 0)));
    }

    #[test]
    fn cloudy_afternoon_waits_for_the_best_hour_left() {
        let config = AutomationConfig { water_end_hour: 16, ..config() };
        let sky = forecast(&[(15, 200.0), (16, 350.0)]);
        let mut memory = WaterMemory::default();
        let early = step(&mut memory, &input(at(14, 10), 85.0, 500.0, 400.0, Some(&sky), &config));
        assert_eq!(early.status.phase, WaterPhase::Waiting);
        let best = step(&mut memory, &input(at(15, 10), 85.0, 500.0, 400.0, Some(&sky), &config));
        assert_eq!(best.status.phase, WaterPhase::Ready);
    }

    #[test]
    fn cloudy_afternoon_needs_the_cloudy_battery_level() {
        let config = config();
        let mut memory = WaterMemory::default();
        let result = step(&mut memory, &input(at(14, 10), 60.0, 500.0, 400.0, None, &config));
        assert_eq!(result.status.phase, WaterPhase::Waiting);
    }

    #[test]
    fn last_call_is_sent_once_and_only_soon_after_the_window() {
        let config = config();
        let mut memory = WaterMemory::default();
        assert!(step(&mut memory, &input(at(15, 5), 100.0, 300.0, 300.0, None, &config)).notify.is_some());
        assert!(step(&mut memory, &input(at(15, 20), 100.0, 300.0, 300.0, None, &config)).notify.is_none());
        let mut late = WaterMemory::default();
        assert!(step(&mut late, &input(at(19, 0), 100.0, 300.0, 300.0, None, &config)).notify.is_none());
    }
}
