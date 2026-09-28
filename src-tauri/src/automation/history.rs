use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::Serialize;

const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const SAMPLE_HEADER: &str = "timestamp,soc_pct,load_w,pv_w,battery_a,battery_v,grid_on,grid_basis,mode,source";
const DECISION_HEADER: &str = "timestamp,window,mode,reserve_soc,recheck_minutes,confidence,dry_run,applied,reason";
const NIGHT_HOURS: [u32; 15] = [17, 18, 19, 20, 21, 22, 23, 0, 1, 2, 3, 4, 5, 6, 7];
const DAY_HOURS: [u32; 14] = [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
const MIN_HOURS_FOR_A_NIGHT: usize = 3;
const FULL_SOC: f64 = 95.0;
const OUTAGE_GAP_MINUTES: i64 = 60;
/// A sample counts as "on battery" when the inverter is on SBG and the BMS
/// shows at least this much discharge.
const ON_BATTERY_A: f64 = 1.0;
const SLEEP_HOURS: std::ops::Range<u32> = 0..6;

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub at: NaiveDateTime,
    pub soc: Option<f64>,
    pub load_w: Option<f64>,
    pub pv_w: Option<f64>,
    pub battery_a: Option<f64>,
    pub battery_v: Option<f64>,
    pub grid_on: Option<bool>,
    pub grid_basis: String,
    pub mode: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecisionRow {
    pub at: NaiveDateTime,
    pub window: String,
    pub mode: String,
    pub reserve_soc: Option<f64>,
    pub recheck_minutes: Option<u32>,
    pub confidence: Option<f64>,
    pub dry_run: bool,
    pub applied: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct HistoryStore {
    root: PathBuf,
}

impl HistoryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn month_file(&self, kind: &str, date: NaiveDate) -> PathBuf {
        self.root.join(kind).join(format!("{:04}-{:02}.csv", date.year(), date.month()))
    }

    pub(crate) fn append(&self, kind: &str, header: &str, date: NaiveDate, line: &str) -> std::io::Result<()> {
        let path = self.month_file(kind, date);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let is_new = !path.exists();
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        if is_new {
            writeln!(file, "{header}")?;
        }
        writeln!(file, "{line}")
    }

    pub fn append_sample(&self, sample: &Sample) -> std::io::Result<()> {
        self.append("samples", SAMPLE_HEADER, sample.at.date(), &sample_to_csv(sample))
    }

    pub fn append_decision(&self, row: &DecisionRow) -> std::io::Result<()> {
        self.append("decisions", DECISION_HEADER, row.at.date(), &decision_to_csv(row))
    }

    pub(crate) fn read_lines(&self, kind: &str, date: NaiveDate) -> Vec<String> {
        fs::read_to_string(self.month_file(kind, date))
            .map(|text| text.lines().skip(1).map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// Samples from the previous and the current month, oldest first.
    pub fn recent_samples(&self, today: NaiveDate) -> Vec<Sample> {
        let mut samples: Vec<Sample> = [previous_month(today), today]
            .iter()
            .flat_map(|date| self.read_lines("samples", *date))
            .filter_map(|line| parse_sample(&line))
            .collect();
        samples.sort_by_key(|sample| sample.at);
        samples
    }

    pub fn recent_decisions(&self, today: NaiveDate, limit: usize) -> Vec<DecisionRow> {
        let mut rows: Vec<DecisionRow> = [previous_month(today), today]
            .iter()
            .flat_map(|date| self.read_lines("decisions", *date))
            .filter_map(|line| parse_decision(&line))
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.at));
        rows.truncate(limit);
        rows
    }

    pub fn rows_this_month(&self, kind: &str, today: NaiveDate) -> usize {
        self.read_lines(kind, today).len()
    }
}

pub(crate) fn previous_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).and_then(|first| first.pred_opt()).unwrap_or(date)
}

pub(crate) fn num(value: Option<f64>, digits: usize) -> String {
    value.map(|v| format!("{v:.digits$}")).unwrap_or_default()
}

pub(crate) fn quote(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    if flat.contains([',', '"']) {
        format!("\"{}\"", flat.replace('"', "\"\""))
    } else {
        flat
    }
}

fn sample_to_csv(sample: &Sample) -> String {
    [
        sample.at.format(TIMESTAMP_FORMAT).to_string(),
        num(sample.soc, 1),
        num(sample.load_w, 0),
        num(sample.pv_w, 0),
        num(sample.battery_a, 2),
        num(sample.battery_v, 2),
        sample.grid_on.map(|on| if on { "1" } else { "0" }.to_string()).unwrap_or_default(),
        quote(&sample.grid_basis),
        quote(sample.mode.as_deref().unwrap_or("")),
        quote(&sample.source),
    ]
    .join(",")
}

fn decision_to_csv(row: &DecisionRow) -> String {
    [
        row.at.format(TIMESTAMP_FORMAT).to_string(),
        quote(&row.window),
        quote(&row.mode),
        num(row.reserve_soc, 0),
        row.recheck_minutes.map(|m| m.to_string()).unwrap_or_default(),
        num(row.confidence, 2),
        (row.dry_run as u8).to_string(),
        (row.applied as u8).to_string(),
        quote(&row.reason),
    ]
    .join(",")
}

pub(crate) fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

pub(crate) fn opt_f64(text: &str) -> Option<f64> {
    text.trim().parse().ok().filter(|v: &f64| v.is_finite())
}

fn opt_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_sample(line: &str) -> Option<Sample> {
    let f = split_csv_line(line);
    if f.len() < 10 {
        return None;
    }
    Some(Sample {
        at: NaiveDateTime::parse_from_str(f[0].trim(), TIMESTAMP_FORMAT).ok()?,
        soc: opt_f64(&f[1]),
        load_w: opt_f64(&f[2]),
        pv_w: opt_f64(&f[3]),
        battery_a: opt_f64(&f[4]),
        battery_v: opt_f64(&f[5]),
        grid_on: match f[6].trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        },
        grid_basis: f[7].trim().to_string(),
        mode: opt_text(&f[8]),
        source: f[9].trim().to_string(),
    })
}

fn parse_decision(line: &str) -> Option<DecisionRow> {
    let f = split_csv_line(line);
    if f.len() < 9 {
        return None;
    }
    Some(DecisionRow {
        at: NaiveDateTime::parse_from_str(f[0].trim(), TIMESTAMP_FORMAT).ok()?,
        window: f[1].trim().to_string(),
        mode: f[2].trim().to_string(),
        reserve_soc: opt_f64(&f[3]),
        recheck_minutes: f[4].trim().parse().ok(),
        confidence: opt_f64(&f[5]),
        dry_run: f[6].trim() == "1",
        applied: f[7].trim() == "1",
        reason: f[8].clone(),
    })
}

/// Evening hours (from 17:00) belong to that date's night; early-morning hours to the
/// previous date's night; midday belongs to no night.
pub fn night_key(at: NaiveDateTime) -> Option<NaiveDate> {
    match at.hour() {
        17..=23 => Some(at.date()),
        0..=7 => at.date().pred_opt(),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourLoad {
    pub hour: u32,
    pub load_w: f64,
    pub nights: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NightProfile {
    pub nights_with_data: u32,
    pub hourly: Vec<HourLoad>,
}

impl NightProfile {
    pub fn load_at(&self, hour: u32) -> Option<f64> {
        self.hourly.iter().find(|h| h.hour == hour).map(|h| h.load_w)
    }

    /// First hour after `from_hour` where typical load falls below 60% of the
    /// `from_hour` load: when the house usually goes quiet.
    pub fn quiet_by(&self, from_hour: u32) -> Option<u32> {
        let start = self.load_at(from_hour)?;
        let position = NIGHT_HOURS.iter().position(|h| *h == from_hour)?;
        NIGHT_HOURS[position + 1..]
            .iter()
            .find(|hour| self.load_at(**hour).is_some_and(|load| load < start * 0.6))
            .copied()
    }
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

fn hourly_means(samples: &[&Sample], hours: &[u32]) -> Vec<(u32, f64)> {
    hours
        .iter()
        .filter_map(|hour| {
            let loads: Vec<f64> = samples
                .iter()
                .filter(|s| s.at.hour() == *hour)
                .filter_map(|s| s.load_w)
                .collect();
            (!loads.is_empty()).then(|| (*hour, loads.iter().sum::<f64>() / loads.len() as f64))
        })
        .collect()
}

/// Typical load per night hour: each past night's hourly mean, then the
/// median across the most recent `nights` nights that have enough data.
pub fn night_profile(samples: &[Sample], exclude_night: NaiveDate, nights: usize) -> NightProfile {
    let mut by_night: Vec<(NaiveDate, Vec<(u32, f64)>)> = Vec::new();
    let mut keys: Vec<NaiveDate> = samples
        .iter()
        .filter_map(|s| night_key(s.at))
        .filter(|key| *key < exclude_night)
        .collect();
    keys.sort();
    keys.dedup();
    for key in keys.into_iter().rev() {
        let night_samples: Vec<&Sample> = samples.iter().filter(|s| night_key(s.at) == Some(key)).collect();
        let means = hourly_means(&night_samples, &NIGHT_HOURS);
        if means.len() >= MIN_HOURS_FOR_A_NIGHT {
            by_night.push((key, means));
        }
        if by_night.len() == nights {
            break;
        }
    }
    let hourly = NIGHT_HOURS
        .iter()
        .filter_map(|hour| {
            let mut values: Vec<f64> = by_night
                .iter()
                .filter_map(|(_, means)| means.iter().find(|(h, _)| h == hour).map(|(_, v)| *v))
                .collect();
            let count = values.len() as u32;
            median(&mut values).map(|load_w| HourLoad { hour: *hour, load_w, nights: count })
        })
        .collect();
    NightProfile {
        nights_with_data: by_night.len() as u32,
        hourly,
    }
}

pub fn tonight_hourly(samples: &[Sample], night: NaiveDate) -> Vec<HourLoad> {
    let tonight: Vec<&Sample> = samples.iter().filter(|s| night_key(s.at) == Some(night)).collect();
    hourly_means(&tonight, &NIGHT_HOURS)
        .into_iter()
        .map(|(hour, load_w)| HourLoad { hour, load_w, nights: 1 })
        .collect()
}

/// Typical house load per daytime hour over the last `days` days (median of
/// each day's hourly mean), so one day with an EV charging doesn't skew it.
pub fn day_profile(samples: &[Sample], today: NaiveDate, days: i64) -> Vec<HourLoad> {
    let per_day: Vec<Vec<(u32, f64)>> = (1..=days)
        .filter_map(|back| today.checked_sub_signed(Duration::days(back)))
        .map(|date| {
            let day: Vec<&Sample> = samples.iter().filter(|s| s.at.date() == date).collect();
            hourly_means(&day, &DAY_HOURS)
        })
        .filter(|means| means.len() >= MIN_HOURS_FOR_A_NIGHT)
        .collect();
    DAY_HOURS
        .iter()
        .filter_map(|hour| {
            let mut values: Vec<f64> = per_day
                .iter()
                .filter_map(|means| means.iter().find(|(h, _)| h == hour).map(|(_, v)| *v))
                .collect();
            let count = values.len() as u32;
            median(&mut values).map(|load_w| HourLoad { hour: *hour, load_w, nights: count })
        })
        .collect()
}

pub fn soc_points(samples: &[Sample], since: NaiveDateTime) -> Vec<(NaiveDateTime, f64)> {
    samples
        .iter()
        .filter(|s| s.at >= since)
        .filter_map(|s| s.soc.map(|soc| (s.at, soc)))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outage {
    pub start: NaiveDateTime,
    pub minutes: f64,
    pub ongoing: bool,
}

/// Grid-off stretches since `since`. An outage ends at the first grid-on
/// sample; a data gap longer than an hour closes it at the last off sample.
pub fn outages(samples: &[Sample], since: NaiveDateTime) -> Vec<Outage> {
    let mut events = Vec::new();
    let mut open: Option<(NaiveDateTime, NaiveDateTime)> = None;
    for sample in samples.iter().filter(|s| s.at >= since) {
        if let Some((start, last)) = open {
            if sample.at - last > Duration::minutes(OUTAGE_GAP_MINUTES) {
                events.push(Outage { start, minutes: minutes_between(start, last), ongoing: false });
                open = None;
            }
        }
        match (sample.grid_on, open) {
            (Some(false), None) => open = Some((sample.at, sample.at)),
            (Some(false), Some((start, _))) => open = Some((start, sample.at)),
            (Some(true), Some((start, _))) => {
                events.push(Outage { start, minutes: minutes_between(start, sample.at), ongoing: false });
                open = None;
            }
            _ => {}
        }
    }
    if let Some((start, last)) = open {
        events.push(Outage { start, minutes: minutes_between(start, last), ongoing: true });
    }
    events
}

fn minutes_between(start: NaiveDateTime, end: NaiveDateTime) -> f64 {
    (end - start).num_seconds() as f64 / 60.0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DaySolar {
    pub date: NaiveDate,
    pub max_soc: Option<f64>,
    pub full_at: Option<NaiveTime>,
    pub samples: usize,
}

pub fn daily_solar(samples: &[Sample], today: NaiveDate, days: i64) -> Vec<DaySolar> {
    (1..=days)
        .rev()
        .filter_map(|back| today.checked_sub_signed(Duration::days(back)))
        .filter_map(|date| {
            let daytime: Vec<&Sample> = samples
                .iter()
                .filter(|s| s.at.date() == date && (6..20).contains(&s.at.hour()))
                .collect();
            if daytime.is_empty() {
                return None;
            }
            let max_soc = daytime.iter().filter_map(|s| s.soc).reduce(f64::max);
            let full_at = daytime.iter().find(|s| s.soc.is_some_and(|soc| soc >= FULL_SOC)).map(|s| s.at.time());
            Some(DaySolar { date, max_soc, full_at, samples: daytime.len() })
        })
        .collect()
}

/// How one recorded night actually went on battery, so the agent can see
/// whether earlier starts and reserves worked out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NightOutcome {
    pub night: NaiveDate,
    pub battery_from: Option<NaiveDateTime>,
    pub soc_at_battery_start: Option<f64>,
    pub battery_until: Option<NaiveDateTime>,
    pub lowest_soc: Option<f64>,
    pub morning_soc: Option<f64>,
    /// Mean battery draw while on battery in the evening (17:00–23:59).
    pub evening_draw_a: Option<f64>,
    /// Mean battery draw while on battery after midnight (00:00–05:59).
    pub sleep_draw_a: Option<f64>,
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0usize), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / count as f64)
}

/// The last `nights` recorded nights before `exclude_night`, newest first.
pub fn night_outcomes(samples: &[Sample], exclude_night: NaiveDate, nights: usize) -> Vec<NightOutcome> {
    let mut by_night: std::collections::BTreeMap<NaiveDate, Vec<&Sample>> = std::collections::BTreeMap::new();
    for sample in samples {
        if let Some(night) = night_key(sample.at).filter(|night| *night < exclude_night) {
            by_night.entry(night).or_default().push(sample);
        }
    }
    by_night
        .into_iter()
        .rev()
        .take(nights)
        .map(|(night, samples)| {
            let battery: Vec<&Sample> = samples
                .iter()
                .copied()
                .filter(|s| s.mode.as_deref() == Some("SBG") && s.battery_a.is_some_and(|amps| amps < -ON_BATTERY_A))
                .collect();
            let draw = |hours: &dyn Fn(u32) -> bool| {
                mean(battery.iter().filter(|s| hours(s.at.hour())).filter_map(|s| s.battery_a.map(|amps| -amps)))
            };
            NightOutcome {
                night,
                battery_from: battery.first().map(|s| s.at),
                soc_at_battery_start: battery.first().and_then(|s| s.soc),
                battery_until: battery.last().map(|s| s.at),
                lowest_soc: samples.iter().filter_map(|s| s.soc).reduce(f64::min),
                morning_soc: samples.iter().rev().find_map(|s| s.soc),
                evening_draw_a: draw(&|hour| hour >= 17),
                sleep_draw_a: draw(&|hour| SLEEP_HOURS.contains(&hour)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn sample(when: NaiveDateTime, load: f64, soc: f64, grid_on: Option<bool>) -> Sample {
        Sample {
            at: when,
            soc: Some(soc),
            load_w: Some(load),
            pv_w: Some(0.0),
            battery_a: Some(-5.0),
            battery_v: Some(51.2),
            grid_on,
            grid_basis: "ac_input".into(),
            mode: Some("Solar".into()),
            source: "inverter+bms".into(),
        }
    }

    fn night_of(day: u32, evening_load: f64, sleep_load: f64) -> Vec<Sample> {
        let mut samples = Vec::new();
        for hour in 18..=23 {
            let load = if hour < 23 { evening_load } else { sleep_load };
            samples.push(sample(at(day, hour, 0), load, 80.0, Some(true)));
            samples.push(sample(at(day, hour, 30), load, 80.0, Some(true)));
        }
        for hour in 0..=6 {
            samples.push(sample(at(day + 1, hour, 0), sleep_load, 60.0, Some(true)));
        }
        samples
    }

    #[test]
    fn csv_round_trips_samples_and_quoted_decisions() {
        let s = sample(at(20, 21, 5), 612.0, 71.0, Some(false));
        assert_eq!(parse_sample(&sample_to_csv(&s)), Some(s.clone()));

        let row = DecisionRow {
            at: at(20, 21, 5),
            window: "night".into(),
            mode: "sbg".into(),
            reserve_soc: Some(35.0),
            recheck_minutes: Some(60),
            confidence: Some(0.8),
            dry_run: true,
            applied: false,
            reason: "Family awake, \"quiet\" by 23:00, sunny tomorrow".into(),
        };
        assert_eq!(parse_decision(&decision_to_csv(&row)), Some(row));
    }

    #[test]
    fn store_appends_with_header_and_reads_back() {
        let dir = std::env::temp_dir().join(format!("solar-hub-history-{}", uuid::Uuid::new_v4()));
        let store = HistoryStore::new(dir.clone());
        store.append_sample(&sample(at(20, 21, 0), 500.0, 70.0, Some(true))).unwrap();
        store.append_sample(&sample(at(20, 21, 5), 520.0, 69.0, Some(true))).unwrap();
        let text = fs::read_to_string(dir.join("samples").join("2026-09.csv")).unwrap();
        assert!(text.starts_with(SAMPLE_HEADER));
        assert_eq!(store.recent_samples(at(20, 22, 0).date()).len(), 2);
        assert_eq!(store.rows_this_month("samples", at(20, 22, 0).date()), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn night_key_groups_evening_with_the_following_morning() {
        assert_eq!(night_key(at(20, 22, 0)), Some(at(20, 0, 0).date()));
        assert_eq!(night_key(at(21, 3, 0)), Some(at(20, 0, 0).date()));
        assert_eq!(night_key(at(21, 12, 0)), None);
    }

    #[test]
    fn profile_takes_the_median_across_nights_and_finds_quiet_hour() {
        let mut samples = night_of(10, 900.0, 250.0);
        samples.extend(night_of(11, 700.0, 300.0));
        samples.extend(night_of(12, 800.0, 200.0));
        let profile = night_profile(&samples, at(13, 0, 0).date(), 7);
        assert_eq!(profile.nights_with_data, 3);
        assert_eq!(profile.load_at(21), Some(800.0));
        assert_eq!(profile.load_at(2), Some(250.0));
        assert_eq!(profile.quiet_by(21), Some(23));
    }

    #[test]
    fn profile_excludes_tonight_and_sparse_nights() {
        let mut samples = night_of(10, 900.0, 250.0);
        samples.push(sample(at(11, 19, 0), 5000.0, 80.0, Some(true)));
        samples.extend(night_of(12, 600.0, 200.0));
        let profile = night_profile(&samples, at(12, 0, 0).date(), 7);
        assert_eq!(profile.nights_with_data, 1, "night 11 is too sparse, night 12 is tonight");
        assert_eq!(profile.load_at(21), Some(900.0));
    }

    #[test]
    fn outages_close_on_grid_back_or_long_gaps() {
        let samples = vec![
            sample(at(20, 22, 0), 400.0, 70.0, Some(true)),
            sample(at(20, 22, 15), 400.0, 69.0, Some(false)),
            sample(at(20, 22, 30), 400.0, 68.0, Some(false)),
            sample(at(20, 23, 15), 400.0, 67.0, Some(true)),
            sample(at(21, 1, 0), 400.0, 60.0, Some(false)),
            sample(at(21, 4, 0), 400.0, 55.0, Some(true)),
            sample(at(21, 5, 0), 400.0, 55.0, Some(false)),
        ];
        let events = outages(&samples, at(20, 0, 0));
        assert_eq!(events.len(), 3);
        assert_eq!((events[0].start, events[0].minutes), (at(20, 22, 15), 60.0));
        assert_eq!(events[1].minutes, 0.0, "gap over an hour closes at the last off sample");
        assert!(events[2].ongoing);
    }

    #[test]
    fn day_profile_ignores_a_single_heavy_day() {
        let mut samples = Vec::new();
        for day in 20..=24 {
            for hour in 9..=14 {
                let load = if day == 22 { 3500.0 } else { 400.0 };
                samples.push(sample(at(day, hour, 0), load, 70.0, Some(true)));
            }
        }
        let profile = day_profile(&samples, at(25, 0, 0).date(), 7);
        let noon = profile.iter().find(|h| h.hour == 12).unwrap();
        assert_eq!(noon.load_w, 400.0);
        assert_eq!(noon.nights, 5);
    }

    #[test]
    fn daily_solar_finds_when_battery_filled() {
        let samples = vec![
            sample(at(19, 9, 0), 300.0, 60.0, Some(true)),
            sample(at(19, 13, 30), 300.0, 96.0, Some(true)),
            sample(at(19, 15, 0), 300.0, 100.0, Some(true)),
            sample(at(20, 12, 0), 300.0, 70.0, Some(true)),
        ];
        let days = daily_solar(&samples, at(21, 0, 0).date(), 3);
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].full_at, NaiveTime::from_hms_opt(13, 30, 0));
        assert_eq!(days[0].max_soc, Some(100.0));
        assert_eq!(days[1].full_at, None);
    }

    fn battery_sample(when: NaiveDateTime, soc: f64, amps: f64, mode: &str) -> Sample {
        Sample {
            at: when,
            soc: Some(soc),
            load_w: Some(amps.abs() * 25.0),
            pv_w: Some(0.0),
            battery_a: Some(amps),
            battery_v: Some(26.0),
            grid_on: Some(true),
            grid_basis: "ac_input".into(),
            mode: Some(mode.into()),
            source: "inverter+bms".into(),
        }
    }

    #[test]
    fn night_outcomes_describe_when_the_battery_ran_and_how_hard() {
        let samples = vec![
            battery_sample(at(26, 18, 0), 95.0, 0.5, "Solar"),
            battery_sample(at(26, 19, 30), 94.0, -14.0, "SBG"),
            battery_sample(at(26, 22, 0), 80.0, -16.0, "SBG"),
            battery_sample(at(27, 2, 0), 55.0, -4.0, "SBG"),
            battery_sample(at(27, 4, 0), 48.0, -4.2, "SBG"),
            battery_sample(at(27, 5, 30), 46.0, -0.8, "Solar"),
            battery_sample(at(27, 19, 0), 90.0, -12.0, "SBG"),
        ];
        let outcomes = night_outcomes(&samples, at(27, 0, 0).date(), 7);
        assert_eq!(outcomes.len(), 1, "tonight is left out");
        let night = &outcomes[0];
        assert_eq!(night.night, at(26, 0, 0).date());
        assert_eq!(night.battery_from, Some(at(26, 19, 30)));
        assert_eq!(night.soc_at_battery_start, Some(94.0));
        assert_eq!(night.battery_until, Some(at(27, 4, 0)));
        assert_eq!(night.lowest_soc, Some(46.0));
        assert_eq!(night.morning_soc, Some(46.0));
        assert!((night.evening_draw_a.unwrap() - 15.0).abs() < 1e-9);
        assert!((night.sleep_draw_a.unwrap() - 4.1).abs() < 1e-9);
    }
}
