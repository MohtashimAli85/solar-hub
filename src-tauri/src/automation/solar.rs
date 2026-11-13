//! Today's expected panel output by time of day, for the dashboard's sun chart.
//!
//! One factor over the whole day overshoots at the edges: low sun and shade
//! cut the panels far more than the flat-ground forecast says. So the first
//! and last hours learn their own factor per half hour, and the readings of
//! the last half hour pull the coming hours towards what the sky is really
//! doing: a darker sky is kept for most of the day, a brighter one fades fast.

use chrono::{Duration, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::automation::history::Sample;
use crate::automation::weather::Forecast;

const SLOTS: usize = 48;
const EDGE_HOURS: f64 = 2.5;
const SLOT_MIN_SAMPLES: usize = 8;
const SLOT_PERCENTILE: f64 = 0.6;
const MIN_RADIATION: f64 = 5.0;
const LIVE_WINDOW_MINUTES: i64 = 30;
const LIVE_MIN_EXPECTED_W: f64 = 300.0;
const BRIGHT_FADE_MINUTES: f64 = 150.0;
const DARK_FADE_MINUTES: f64 = 360.0;
const LIVE_RANGE: (f64, f64) = (0.15, 1.5);
const SURELY_NOT_FULL_SOC: f64 = 90.0;
const STEP_MINUTES: i64 = 15;
const HELD_BACK_SOC: f64 = 95.0;
const NOT_FULL_VOLTS: f64 = 26.4;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PvPoint {
    pub at: NaiveDateTime,
    pub watts: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasuredPv {
    pub at: NaiveDateTime,
    pub watts: f64,
    /// The battery was full, so the inverter may have held the panels back.
    pub held_back: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PvShape {
    ratio: f64,
    slots: [Option<f64>; SLOTS],
    max_w: Option<f64>,
}

fn free_running(sample: &Sample) -> bool {
    match sample.soc {
        Some(soc) => soc < HELD_BACK_SOC,
        None => sample.battery_v.is_some_and(|volts| volts < NOT_FULL_VOLTS),
    }
}

fn slot_of(at: NaiveDateTime) -> usize {
    (at.hour() * 2 + u32::from(at.minute() >= 30)) as usize
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}

impl PvShape {
    pub fn learn(
        samples: &[Sample],
        forecast: &Forecast,
        pv_array_watts: f64,
        ratio: f64,
        since: NaiveDateTime,
        sunrise: NaiveDateTime,
        sunset: NaiveDateTime,
    ) -> Self {
        let mut per_slot: Vec<Vec<f64>> = vec![Vec::new(); SLOTS];
        let mut max_w: Option<f64> = None;
        for sample in samples.iter().filter(|s| s.at >= since) {
            let Some(pv) = sample.pv_w else { continue };
            max_w = Some(max_w.map_or(pv, |max| max.max(pv)));
            if !free_running(sample) || pv_array_watts <= 0.0 {
                continue;
            }
            let Some(radiation) = forecast.radiation_at(sample.at).filter(|r| *r >= MIN_RADIATION) else { continue };
            per_slot[slot_of(sample.at)].push(pv / (radiation * pv_array_watts / 1000.0));
        }
        let minutes = |at: NaiveDateTime| f64::from(at.hour() * 60 + at.minute());
        let (rise, set) = (minutes(sunrise), minutes(sunset));
        let edge = EDGE_HOURS * 60.0;
        let mut slots = [None; SLOTS];
        for (index, slot) in slots.iter_mut().enumerate() {
            let centre = index as f64 * 30.0 + 15.0;
            if centre >= rise + edge && centre <= set - edge {
                continue;
            }
            if per_slot[index].len() >= SLOT_MIN_SAMPLES {
                *slot = Some(percentile(per_slot[index].clone(), SLOT_PERCENTILE).min(ratio));
            }
        }
        Self { ratio, slots, max_w }
    }

    fn ratio_at(&self, at: NaiveDateTime) -> f64 {
        let minutes = f64::from(at.hour() * 60 + at.minute()) - 15.0;
        let index = (minutes / 30.0).floor();
        let frac = (minutes - index * 30.0) / 30.0;
        let get = |i: f64| (0.0..SLOTS as f64).contains(&i).then(|| self.slots[i as usize]).flatten().unwrap_or(self.ratio);
        let (a, b) = (get(index), get(index + 1.0));
        a + (b - a) * frac
    }

    pub fn watts(&self, forecast: &Forecast, pv_array_watts: f64, at: NaiveDateTime) -> Option<f64> {
        let radiation = forecast.radiation_at(at)?;
        let watts = radiation * pv_array_watts / 1000.0 * self.ratio_at(at);
        Some(self.max_w.map_or(watts, |max| watts.min(max)))
    }
}

/// How much brighter or darker the sky is now than forecast, from the last
/// half hour's readings. Only samples whose output the battery didn't hold
/// back can say "darker"; any reading can say "brighter". A sample without
/// its own SOC counts as not held back when the latest SOC in the window is
/// well below full.
pub fn live_factor(shape: &PvShape, forecast: &Forecast, pv_array_watts: f64, samples: &[Sample], now: NaiveDateTime) -> Option<f64> {
    let since = now - Duration::minutes(LIVE_WINDOW_MINUTES);
    let window: Vec<&Sample> = samples.iter().filter(|s| s.at > since && s.at <= now).collect();
    let not_full = window.iter().rev().find_map(|s| s.soc).is_some_and(|soc| soc < SURELY_NOT_FULL_SOC);
    let ratios: Vec<f64> = window
        .iter()
        .filter_map(|s| {
            let pv = s.pv_w?;
            let expected = shape.watts(forecast, pv_array_watts, s.at).filter(|w| *w >= LIVE_MIN_EXPECTED_W)?;
            let free = if s.soc.is_some() { free_running(s) } else { not_full || free_running(s) };
            (free || pv > expected).then_some(pv / expected)
        })
        .collect();
    let latest = *ratios.last()?;
    let median = percentile(ratios, 0.5);
    Some(((median + latest) / 2.0).clamp(LIVE_RANGE.0, LIVE_RANGE.1))
}

pub fn expected_today(shape: &PvShape, forecast: &Forecast, pv_array_watts: f64, factor: Option<f64>, now: NaiveDateTime) -> Vec<PvPoint> {
    if pv_array_watts <= 0.0 {
        return Vec::new();
    }
    let factor = factor.unwrap_or(1.0);
    let fade = if factor < 1.0 { DARK_FADE_MINUTES } else { BRIGHT_FADE_MINUTES };
    let start = now.date().and_hms_opt(0, 0, 0).expect("midnight");
    (0..24 * 60 / STEP_MINUTES)
        .filter_map(|step| {
            let at = start + Duration::minutes(step * STEP_MINUTES);
            let base = shape.watts(forecast, pv_array_watts, at)?;
            let ahead = (at - now).num_minutes() as f64;
            let weight = if ahead > 0.0 { (-ahead / fade).exp() } else { 0.0 };
            let watts = base * (1.0 + (factor - 1.0) * weight);
            Some(PvPoint { at, watts: shape.max_w.map_or(watts, |max| watts.min(max)).round() })
        })
        .collect()
}

/// The curve's watts at `at`, read between its points.
pub fn curve_at(points: &[PvPoint], at: NaiveDateTime) -> Option<f64> {
    let index = points.partition_point(|p| p.at <= at);
    let before = points.get(index.checked_sub(1)?)?;
    let Some(after) = points.get(index) else {
        return (before.at == at).then_some(before.watts);
    };
    let frac = (at - before.at).num_seconds() as f64 / (after.at - before.at).num_seconds() as f64;
    Some(before.watts + (after.watts - before.watts) * frac)
}

pub fn measured_today(samples: &[Sample], now: NaiveDateTime) -> Vec<MeasuredPv> {
    samples
        .iter()
        .filter(|s| s.at.date() == now.date() && s.at <= now)
        .filter_map(|s| {
            Some(MeasuredPv { at: s.at, watts: s.pv_w?.round(), held_back: s.soc.is_some_and(|soc| soc >= HELD_BACK_SOC) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::weather::HourWeather;
    use chrono::NaiveDate;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    /// A clear-ish day: radiation rises linearly from 6:00 to 600 W/m² at
    /// 12:00 and falls back to zero at 18:00, for the 1st to the 10th.
    fn forecast() -> Forecast {
        let mut hourly = Vec::new();
        for day in 1..=10 {
            for hour in 0..24 {
                let mid = f64::from(hour) - 0.5;
                let radiation = (600.0 * (1.0 - (mid - 12.0).abs() / 6.0)).max(0.0);
                hourly.push(HourWeather { at: at(day, hour, 0), temp_c: None, cloud_pct: None, radiation_w_m2: Some(radiation) });
            }
        }
        Forecast { hourly, daily: Vec::new() }
    }

    fn sample(at: NaiveDateTime, pv: f64, soc: f64) -> Sample {
        Sample {
            at,
            soc: Some(soc),
            load_w: Some(300.0),
            pv_w: Some(pv),
            battery_a: Some(10.0),
            battery_v: Some(26.0),
            grid_on: Some(false),
            grid_basis: String::new(),
            mode: None,
            source: String::new(),
        }
    }

    /// Panels that give only a third of the whole-day factor before 8:00.
    fn history(forecast: &Forecast) -> Vec<Sample> {
        let mut samples = Vec::new();
        for day in 1..=7 {
            let mut t = at(day, 6, 30);
            while t < at(day, 12, 0) {
                let radiation = forecast.radiation_at(t).unwrap();
                let ratio = if t.hour() < 8 { 0.5 } else { 1.5 };
                samples.push(sample(t, radiation * 3.0 * ratio, 50.0));
                t += Duration::minutes(10);
            }
        }
        samples
    }

    fn shape(forecast: &Forecast, samples: &[Sample]) -> PvShape {
        PvShape::learn(samples, forecast, 3000.0, 1.5, at(1, 0, 0), at(8, 6, 40), at(8, 17, 10))
    }

    #[test]
    fn early_hours_learn_their_own_lower_factor() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let early = shape.watts(&forecast, 3000.0, at(8, 7, 15)).unwrap();
        let plain = forecast.radiation_at(at(8, 7, 15)).unwrap() * 3.0 * 1.5;
        assert!(early < plain * 0.5, "{early} vs {plain}");
        let noon = shape.watts(&forecast, 3000.0, at(8, 12, 0)).unwrap();
        let full = forecast.radiation_at(at(8, 12, 0)).unwrap() * 3.0 * 1.5;
        assert!((noon - full.min(shape.max_w.unwrap())).abs() < 1.0, "{noon} vs {full}");
    }

    #[test]
    fn output_never_goes_above_the_best_reading() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let best = history(&forecast).iter().filter_map(|s| s.pv_w).fold(0.0, f64::max);
        let points = expected_today(&shape, &forecast, 3000.0, None, at(8, 6, 0));
        assert!(points.iter().all(|p| p.watts <= best.round()));
        assert_eq!(points.len(), 96);
    }

    #[test]
    fn a_dark_morning_pulls_the_next_hours_down_and_fades() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let now = at(8, 9, 0);
        let dark: Vec<Sample> = (0..6)
            .map(|i| {
                let t = now - Duration::minutes(i * 10);
                sample(t, shape.watts(&forecast, 3000.0, t).unwrap() * 0.5, 40.0)
            })
            .collect();
        let plain = expected_today(&shape, &forecast, 3000.0, None, now);
        let factor = live_factor(&shape, &forecast, 3000.0, &dark, now);
        let live = expected_today(&shape, &forecast, 3000.0, factor, now);
        let find = |points: &[PvPoint], t: NaiveDateTime| points.iter().find(|p| p.at == t).unwrap().watts;
        let soon = find(&live, at(8, 9, 15)) / find(&plain, at(8, 9, 15));
        let later = find(&live, at(8, 14, 0)) / find(&plain, at(8, 14, 0));
        assert!(soon < 0.6, "{soon}");
        assert!(later > 0.65 && later < 0.85, "{later}");
        assert_eq!(find(&live, at(8, 8, 0)), find(&plain, at(8, 8, 0)));
    }

    #[test]
    fn a_bright_spell_fades_back_to_the_forecast_fast() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let now = at(8, 9, 0);
        let plain = expected_today(&shape, &forecast, 3000.0, None, now);
        let live = expected_today(&shape, &forecast, 3000.0, Some(1.3), now);
        let find = |points: &[PvPoint], t: NaiveDateTime| points.iter().find(|p| p.at == t).unwrap().watts;
        let later = find(&live, at(8, 14, 0)) / find(&plain, at(8, 14, 0));
        assert!(later < 1.05, "{later}");
    }

    #[test]
    fn a_reading_without_soc_counts_when_the_battery_is_clearly_not_full() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let now = at(8, 9, 30);
        let expected = |t: NaiveDateTime| shape.watts(&forecast, 3000.0, t).unwrap();
        let mut charging = sample(now - Duration::minutes(20), expected(now - Duration::minutes(20)) * 0.3, 0.0);
        charging.soc = None;
        charging.battery_v = Some(27.0);
        assert_eq!(live_factor(&shape, &forecast, 3000.0, std::slice::from_ref(&charging), now), None);
        let latest = sample(now, expected(now) * 0.3, 46.0);
        let factor = live_factor(&shape, &forecast, 3000.0, &[charging, latest], now).unwrap();
        assert!((factor - 0.3).abs() < 0.02, "{factor}");
    }

    #[test]
    fn curve_at_reads_between_points() {
        let points = vec![PvPoint { at: at(8, 9, 0), watts: 400.0 }, PvPoint { at: at(8, 9, 15), watts: 800.0 }];
        assert_eq!(curve_at(&points, at(8, 9, 0)), Some(400.0));
        assert_eq!(curve_at(&points, at(8, 9, 7) + Duration::seconds(30)), Some(600.0));
        assert_eq!(curve_at(&points, at(8, 9, 15)), Some(800.0));
        assert_eq!(curve_at(&points, at(8, 8, 0)), None);
        assert_eq!(curve_at(&points, at(8, 9, 30)), None);
    }

    #[test]
    fn a_full_battery_cannot_say_the_sky_is_darker() {
        let forecast = forecast();
        let shape = shape(&forecast, &history(&forecast));
        let now = at(8, 11, 0);
        let held: Vec<Sample> = (0..6).map(|i| sample(now - Duration::minutes(i * 10), 300.0, 100.0)).collect();
        assert_eq!(live_factor(&shape, &forecast, 3000.0, &held, now), None);
    }

    #[test]
    fn measured_keeps_only_today_and_flags_a_full_battery() {
        let samples = vec![sample(at(7, 12, 0), 2000.0, 60.0), sample(at(8, 9, 0), 900.0, 60.0), sample(at(8, 11, 0), 400.0, 100.0)];
        let measured = measured_today(&samples, at(8, 12, 0));
        assert_eq!(measured.len(), 2);
        assert!(!measured[0].held_back && measured[1].held_back);
    }
}

