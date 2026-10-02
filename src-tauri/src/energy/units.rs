use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use chrono_tz::Tz;
use serde::Serialize;

use crate::inverter::client::HistoryPoint;
use crate::inverter::types::MAINS_PRESENT_VOLTS;

pub const HISTORY_KEYS: [&str; 5] = [
    "load_power",
    "pvInputPower",
    "batteryDischargeCurrent",
    "batteryVoltage",
    "acInputVoltage",
];
const MAX_POINT_SPAN_MINUTES: i64 = 10;
const LAST_POINT_SPAN_MINUTES: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitPoint {
    pub at: NaiveDateTime,
    pub house_w: f64,
    pub pv_w: f64,
    pub discharge_w: f64,
    pub grid_on: bool,
}

/// Local-time points with the values the unit count needs; points missing the
/// house load or the grid voltage can't be attributed and are dropped.
pub fn unit_points(points: &[HistoryPoint], tz: Tz) -> Vec<UnitPoint> {
    let mut converted: Vec<UnitPoint> = points
        .iter()
        .filter_map(|point| {
            let house_kw = point.value("load_power")?;
            let ac_volts = point.value("acInputVoltage")?;
            let discharge_w = point.value("batteryDischargeCurrent").unwrap_or(0.0)
                * point.value("batteryVoltage").unwrap_or(0.0);
            Some(UnitPoint {
                at: point.at.with_timezone(&tz).naive_local().with_nanosecond(0)?,
                house_w: (house_kw * 1000.0).max(0.0),
                pv_w: (point.value("pvInputPower").unwrap_or(0.0) * 1000.0).max(0.0),
                discharge_w: discharge_w.max(0.0),
                grid_on: ac_volts >= MAINS_PRESENT_VOLTS,
            })
        })
        .collect();
    converted.sort_by_key(|point| point.at);
    converted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MeterSwitch {
    pub at: NaiveDateTime,
    pub meter: u8,
}

/// The meter the changeover switch was on at `at`, from switches sorted oldest
/// first; `None` before the first recorded switch.
pub fn meter_at(switches: &[MeterSwitch], at: NaiveDateTime) -> Option<u8> {
    switches.iter().take_while(|switch| switch.at <= at).last().map(|switch| switch.meter)
}

/// A stretch of past time the homeowner says was on one meter. It moves the
/// units nobody assigned in that stretch to the meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MeterAssignment {
    pub from: NaiveDateTime,
    pub to: NaiveDateTime,
    pub meter: u8,
}

impl MeterAssignment {
    pub fn overlaps(&self, other: &MeterAssignment) -> bool {
        self.from < other.to && other.from < self.to
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct GridSplit {
    pub grid_kwh: f64,
    pub meter1_kwh: f64,
    pub meter2_kwh: f64,
    pub unassigned_kwh: f64,
}

impl GridSplit {
    fn add(&mut self, meter: Option<u8>, kwh: f64) {
        self.grid_kwh += kwh;
        match meter {
            Some(1) => self.meter1_kwh += kwh,
            Some(2) => self.meter2_kwh += kwh,
            _ => self.unassigned_kwh += kwh,
        }
    }

    fn scaled(&self, factor: f64) -> GridSplit {
        GridSplit {
            grid_kwh: self.grid_kwh * factor,
            meter1_kwh: self.meter1_kwh * factor,
            meter2_kwh: self.meter2_kwh * factor,
            unassigned_kwh: self.unassigned_kwh * factor,
        }
    }

    fn plus(mut self, other: &GridSplit) -> GridSplit {
        self.grid_kwh += other.grid_kwh;
        self.meter1_kwh += other.meter1_kwh;
        self.meter2_kwh += other.meter2_kwh;
        self.unassigned_kwh += other.unassigned_kwh;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayUnits {
    pub date: NaiveDate,
    #[serde(flatten)]
    pub grid: GridSplit,
    pub house_kwh: f64,
    pub solar_kwh: f64,
    pub battery_kwh: f64,
    pub grid_off_minutes: f64,
    pub data_hours: f64,
    /// The inverter's own draw from the grid, already included in `grid`.
    pub standby_kwh: f64,
}

impl DayUnits {
    pub fn empty(date: NaiveDate) -> Self {
        Self {
            date,
            grid: GridSplit::default(),
            house_kwh: 0.0,
            solar_kwh: 0.0,
            battery_kwh: 0.0,
            grid_off_minutes: 0.0,
            data_hours: 0.0,
            standby_kwh: 0.0,
        }
    }

    pub fn has_data(&self) -> bool {
        self.house_kwh > 0.0
    }

    pub fn grid_on_hours(&self) -> f64 {
        (self.data_hours - self.grid_off_minutes / 60.0).max(0.0)
    }

    /// Adds the inverter's own draw from the grid, which its load reading never
    /// shows, for every hour the grid was up. It goes to the day's meters in
    /// the same shares as the rest of the day's units.
    pub fn with_standby(mut self, watts: f64) -> DayUnits {
        let extra = watts.max(0.0) * self.grid_on_hours() / 1000.0;
        if extra <= 0.0 {
            return self;
        }
        let total = self.grid.grid_kwh;
        let share = |part: f64| if total > 0.0 { part / total } else { 0.0 };
        let (m1, m2) = (share(self.grid.meter1_kwh), share(self.grid.meter2_kwh));
        self.grid.meter1_kwh += extra * m1;
        self.grid.meter2_kwh += extra * m2;
        self.grid.unassigned_kwh += extra * (1.0 - m1 - m2);
        self.grid.grid_kwh += extra;
        self.standby_kwh += extra;
        self
    }

    /// Moves the unassigned units in `window` to the meters in `assignments`,
    /// pro rata by the clock. Only the time before the first changeover is
    /// unassigned, so that is the stretch the assignments are measured against.
    pub fn with_assignments(
        mut self,
        window: (NaiveDateTime, NaiveDateTime),
        first_switch: Option<NaiveDateTime>,
        assignments: &[MeterAssignment],
    ) -> DayUnits {
        let unassigned_until = first_switch.map_or(window.1, |at| at.min(window.1));
        let span = (unassigned_until - window.0).num_seconds() as f64;
        if span <= 0.0 {
            return self;
        }
        let original = self.grid.unassigned_kwh;
        for assignment in assignments {
            let overlap = (assignment.to.min(unassigned_until) - assignment.from.max(window.0)).num_seconds() as f64;
            if overlap <= 0.0 {
                continue;
            }
            let moved = (original * overlap / span).min(self.grid.unassigned_kwh);
            self.grid.unassigned_kwh -= moved;
            match assignment.meter {
                1 => self.grid.meter1_kwh += moved,
                _ => self.grid.meter2_kwh += moved,
            }
        }
        self
    }

    fn scaled(&self, factor: f64) -> DayUnits {
        DayUnits {
            date: self.date,
            grid: self.grid.scaled(factor),
            house_kwh: self.house_kwh * factor,
            solar_kwh: self.solar_kwh * factor,
            battery_kwh: self.battery_kwh * factor,
            grid_off_minutes: self.grid_off_minutes * factor,
            data_hours: self.data_hours * factor,
            standby_kwh: self.standby_kwh * factor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourUnits {
    pub hour: u32,
    pub grid_kwh: f64,
    pub house_kwh: f64,
    pub solar_kwh: f64,
    pub battery_kwh: f64,
}

/// Where each watt of house load came from at one point: solar first, then
/// battery discharge, and the rest from the grid while it is up. During an
/// outage whatever solar didn't cover came from the battery.
fn split_point(point: &UnitPoint) -> (f64, f64, f64) {
    let solar = point.pv_w.min(point.house_w);
    let rest = point.house_w - solar;
    if !point.grid_on {
        return (solar, rest, 0.0);
    }
    let battery = point.discharge_w.min(rest);
    (solar, battery, rest - battery)
}

/// Energy used between `from` and `until`. Each point stands for the time until
/// the next one, capped so a gap in the log doesn't count as hours of the same
/// load, and clipped to the window so a reading time can split a day exactly.
pub fn compute_window(
    points: &[UnitPoint],
    switches: &[MeterSwitch],
    from: NaiveDateTime,
    until: NaiveDateTime,
) -> (DayUnits, Vec<HourUnits>) {
    let day_end = from.date().and_time(NaiveTime::MIN) + Duration::days(1);
    let mut units = DayUnits::empty(from.date());
    let mut hourly: Vec<HourUnits> = (0..24)
        .map(|hour| HourUnits { hour, grid_kwh: 0.0, house_kwh: 0.0, solar_kwh: 0.0, battery_kwh: 0.0 })
        .collect();
    for (index, point) in points.iter().enumerate() {
        let covered_until = match points.get(index + 1) {
            Some(next) => next.at.min(point.at + Duration::minutes(MAX_POINT_SPAN_MINUTES)),
            None => point.at + Duration::minutes(LAST_POINT_SPAN_MINUTES),
        };
        let start = point.at.max(from);
        let end = covered_until.min(until);
        if end <= start {
            continue;
        }
        let hours = (end - start).num_seconds() as f64 / 3600.0;
        let (solar, battery, grid) = split_point(point);
        let kwh = |watts: f64| watts * hours / 1000.0;
        units.grid.add(meter_at(switches, start), kwh(grid));
        units.house_kwh += kwh(point.house_w);
        units.solar_kwh += kwh(solar);
        units.battery_kwh += kwh(battery);
        units.data_hours += hours;
        if !point.grid_on {
            units.grid_off_minutes += hours * 60.0;
        }
        if start < day_end {
            let hour = &mut hourly[start.hour() as usize];
            hour.grid_kwh += kwh(grid);
            hour.house_kwh += kwh(point.house_w);
            hour.solar_kwh += kwh(solar);
            hour.battery_kwh += kwh(battery);
        }
    }
    let hours_so_far = if until < day_end { until.hour() + 1 } else { 24 };
    hourly.truncate(hours_so_far as usize);
    (units, hourly)
}

/// Grid units for one local day, up to `now` if the day isn't over.
pub fn compute_day(
    points: &[UnitPoint],
    switches: &[MeterSwitch],
    day: NaiveDate,
    now: NaiveDateTime,
) -> (DayUnits, Vec<HourUnits>) {
    let day_start = day.and_time(NaiveTime::MIN);
    compute_window(points, switches, day_start, now.min(day_start + Duration::days(1)))
}

/// The UTC window covering one local day, for the history request.
pub fn day_window(day: NaiveDate, tz: Tz) -> Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    let start = tz.from_local_datetime(&day.and_time(NaiveTime::MIN)).earliest()?;
    let end = tz
        .from_local_datetime(&day.and_hms_opt(23, 59, 59)?)
        .latest()?;
    Some((start.with_timezone(&chrono::Utc), end.with_timezone(&chrono::Utc)))
}

/// When the meters are read each month: a day (1–28) and a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub day: u32,
    pub time: NaiveTime,
}

fn shift_months(at: NaiveDateTime, months: i32) -> NaiveDateTime {
    let shifted = if months >= 0 {
        at.checked_add_months(Months::new(months as u32))
    } else {
        at.checked_sub_months(Months::new(months.unsigned_abs()))
    };
    shifted.unwrap_or(at)
}

impl Reading {
    /// The reading that opened the bill period running at `now`.
    pub fn period_start(&self, now: NaiveDateTime) -> NaiveDateTime {
        let date = now.date();
        let this_month = date.with_day(self.day.clamp(1, 28)).unwrap_or(date).and_time(self.time);
        if now >= this_month {
            this_month
        } else {
            shift_months(this_month, -1)
        }
    }

    pub fn previous_period_start(&self, now: NaiveDateTime) -> NaiveDateTime {
        shift_months(self.period_start(now), -1)
    }
}

pub fn next_period_start(start: NaiveDateTime) -> NaiveDateTime {
    shift_months(start, 1)
}

/// A reading day's units before and after the reading time.
#[derive(Debug, Clone, PartialEq)]
pub struct DaySplit {
    pub before: DayUnits,
    pub after: DayUnits,
}

impl DaySplit {
    pub fn with_standby(self, watts: f64) -> DaySplit {
        DaySplit { before: self.before.with_standby(watts), after: self.after.with_standby(watts) }
    }
}

impl DaySplit {
    pub fn with_assignments(self, at: NaiveDateTime, day_end: NaiveDateTime, first_switch: Option<NaiveDateTime>, assignments: &[MeterAssignment]) -> DaySplit {
        let day_start = at.date().and_time(NaiveTime::MIN);
        let cut = at.min(day_end);
        DaySplit {
            before: self.before.with_assignments((day_start, cut), first_switch, assignments),
            after: self.after.with_assignments((cut, day_end), first_switch, assignments),
        }
    }
}

pub fn split_day(points: &[UnitPoint], switches: &[MeterSwitch], at: NaiveDateTime, now: NaiveDateTime) -> DaySplit {
    let day_start = at.date().and_time(NaiveTime::MIN);
    let day_end = (day_start + Duration::days(1)).min(now);
    let (before, _) = compute_window(points, switches, day_start, at.min(day_end));
    let (after, _) = compute_window(points, switches, at, day_end);
    DaySplit { before, after }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BillPeriod {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub days_total: u32,
    pub days_with_data: u32,
    pub so_far: GridSplit,
    pub standby_kwh: f64,
    /// Hours of the period so far that the inverter's log covers, against the
    /// hours that have passed; a gap means units that weren't counted.
    pub log_hours: f64,
    pub elapsed_hours: f64,
    /// For the running period: what it adds up to if the rest of it uses the
    /// grid like the days so far, with the remaining units going to the meter
    /// the switch is on now.
    pub projected: Option<GridSplit>,
}

fn day_fraction(time: NaiveTime) -> f64 {
    time.num_seconds_from_midnight() as f64 / 86_400.0
}

/// Units between one reading and the next. The two reading days count only
/// the part after (or before) the reading time: exactly when that day's log is
/// in `splits`, otherwise pro rata by the clock.
pub fn bill_period(
    days: &[DayUnits],
    splits: &BTreeMap<NaiveDate, DaySplit>,
    start: NaiveDateTime,
    now: NaiveDateTime,
    active_meter: Option<u8>,
) -> BillPeriod {
    let next = next_period_start(start);
    let (first, last) = (start.date(), next.date());
    let mut so_far = GridSplit::default();
    let mut standby_kwh = 0.0;
    let mut log_hours = 0.0;
    let mut days_with_data = 0;
    for day in days.iter().filter(|day| day.date >= first && day.date <= last) {
        let part = if day.date == first {
            splits.get(&first).map(|split| split.after.clone()).unwrap_or_else(|| day.scaled(1.0 - day_fraction(start.time())))
        } else if day.date == last {
            splits.get(&last).map(|split| split.before.clone()).unwrap_or_else(|| day.scaled(day_fraction(next.time())))
        } else {
            day.clone()
        };
        so_far = so_far.plus(&part.grid);
        standby_kwh += part.standby_kwh;
        log_hours += part.data_hours;
        if day.has_data() && day.date < last {
            days_with_data += 1;
        }
    }
    let projected = (now >= start && now < next).then(|| {
        let elapsed_days = (now - start).num_seconds() as f64 / 86_400.0;
        let remaining_days = (next - now).num_seconds() as f64 / 86_400.0;
        let per_day = if elapsed_days >= 0.25 { so_far.grid_kwh / elapsed_days } else { 0.0 };
        let mut projected = so_far;
        projected.add(active_meter, per_day * remaining_days);
        projected
    });
    BillPeriod {
        start,
        end: next,
        days_total: (last - first).num_days() as u32,
        days_with_data,
        so_far,
        standby_kwh,
        log_hours,
        elapsed_hours: (now.min(next) - start).num_seconds().max(0) as f64 / 3600.0,
        projected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn point(when: NaiveDateTime, house_w: f64, pv_w: f64, discharge_w: f64, grid_on: bool) -> UnitPoint {
        UnitPoint { at: when, house_w, pv_w, discharge_w, grid_on }
    }

    /// One point every 5 minutes for an hour starting at `hour`.
    fn hour_of(day: u32, hour: u32, house_w: f64, pv_w: f64, discharge_w: f64, grid_on: bool) -> Vec<UnitPoint> {
        (0..12).map(|i| point(at(day, hour, i * 5), house_w, pv_w, discharge_w, grid_on)).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn day(date: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, date).unwrap()
    }

    #[test]
    fn solar_mode_without_sun_is_all_grid() {
        let points = hour_of(20, 2, 400.0, 0.0, 0.0, true);
        let (units, hourly) = compute_day(&points, &[], day(20), at(21, 0, 0));
        assert!(close(units.grid.grid_kwh, 0.4));
        assert!(close(units.house_kwh, 0.4));
        assert!(close(units.grid.unassigned_kwh, 0.4));
        assert!(close(hourly[2].grid_kwh, 0.4));
        assert_eq!(hourly.len(), 24);
    }

    #[test]
    fn battery_discharge_is_not_counted() {
        let points = hour_of(20, 23, 240.0, 0.0, 330.0, true);
        let (units, _) = compute_day(&points, &[], day(20), at(21, 0, 0));
        assert!(close(units.grid.grid_kwh, 0.0));
        assert!(close(units.battery_kwh, 0.24));
    }

    #[test]
    fn only_the_load_solar_misses_is_grid() {
        let points = hour_of(20, 8, 1000.0, 700.0, 0.0, true);
        let (units, _) = compute_day(&points, &[], day(20), at(21, 0, 0));
        assert!(close(units.grid.grid_kwh, 0.3));
        assert!(close(units.solar_kwh, 0.7));
        let sunny = hour_of(20, 11, 300.0, 1500.0, 0.0, true);
        let (units, _) = compute_day(&sunny, &[], day(20), at(21, 0, 0));
        assert!(close(units.grid.grid_kwh, 0.0));
        assert!(close(units.solar_kwh, 0.3));
    }

    #[test]
    fn outage_counts_nothing_and_the_battery_covers_the_rest() {
        let points = hour_of(20, 12, 1000.0, 700.0, 0.0, false);
        let (units, _) = compute_day(&points, &[], day(20), at(21, 0, 0));
        assert!(close(units.grid.grid_kwh, 0.0));
        assert!(close(units.battery_kwh, 0.3));
        assert!(close(units.grid_off_minutes, 60.0));
    }

    #[test]
    fn gaps_are_capped_and_the_last_point_counts_briefly() {
        let points = vec![point(at(20, 1, 0), 600.0, 0.0, 0.0, true), point(at(20, 1, 30), 600.0, 0.0, 0.0, true)];
        let (units, _) = compute_day(&points, &[], day(20), at(21, 0, 0));
        assert!(close(units.data_hours, 15.0 / 60.0));
        assert!(close(units.grid.grid_kwh, 0.15));
    }

    #[test]
    fn today_stops_at_now_and_trims_future_hours() {
        let points = hour_of(28, 9, 600.0, 0.0, 0.0, true);
        let (units, hourly) = compute_day(&points, &[], day(28), at(28, 9, 30));
        assert!(close(units.data_hours, 0.5));
        assert_eq!(hourly.len(), 10);
    }

    #[test]
    fn a_day_without_load_has_no_data() {
        let (units, hourly) = compute_day(&[], &[], day(20), at(21, 0, 0));
        assert!(!units.has_data());
        assert!(hourly.iter().all(|hour| hour.house_kwh == 0.0));
    }

    #[test]
    fn meter_switch_splits_the_day() {
        let switches = [
            MeterSwitch { at: at(20, 12, 0), meter: 1 },
            MeterSwitch { at: at(20, 14, 0), meter: 2 },
        ];
        assert_eq!(meter_at(&switches, at(20, 11, 59)), None);
        assert_eq!(meter_at(&switches, at(20, 12, 0)), Some(1));
        assert_eq!(meter_at(&switches, at(20, 20, 0)), Some(2));
        let points: Vec<UnitPoint> = (11..15).flat_map(|hour| hour_of(20, hour, 500.0, 0.0, 0.0, true)).collect();
        let (units, _) = compute_day(&points, &switches, day(20), at(21, 0, 0));
        assert!(close(units.grid.unassigned_kwh, 0.5));
        assert!(close(units.grid.meter1_kwh, 1.0));
        assert!(close(units.grid.meter2_kwh, 0.5));
        assert!(close(units.grid.grid_kwh, 2.0));
    }

    fn reading(day: u32, hour: u32, minute: u32) -> Reading {
        Reading { day, time: NaiveTime::from_hms_opt(hour, minute, 0).unwrap() }
    }

    fn on(month: u32, date: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, month, date).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn bill_period_starts_at_the_reading_time() {
        let read = reading(23, 11, 57);
        assert_eq!(read.period_start(on(9, 23, 11, 56)), on(8, 23, 11, 57));
        assert_eq!(read.period_start(on(9, 23, 11, 57)), on(9, 23, 11, 57));
        assert_eq!(read.period_start(on(9, 28, 9, 0)), on(9, 23, 11, 57));
        assert_eq!(read.previous_period_start(on(9, 28, 9, 0)), on(8, 23, 11, 57));
        assert_eq!(next_period_start(on(9, 23, 11, 57)), on(10, 23, 11, 57));
        let january = NaiveDate::from_ymd_opt(2027, 1, 3).unwrap().and_hms_opt(8, 0, 0).unwrap();
        assert_eq!(reading(12, 0, 0).period_start(january), on(12, 12, 0, 0));
    }

    fn day_units(date: NaiveDate, meter1: f64, meter2: f64) -> DayUnits {
        let mut units = DayUnits::empty(date);
        units.grid.add(Some(1), meter1);
        units.grid.add(Some(2), meter2);
        units.house_kwh = 5.0;
        units
    }

    #[test]
    fn bill_period_totals_and_projects_onto_the_active_meter() {
        let days: Vec<DayUnits> = (1..=10).map(|d| day_units(day(d), 1.0, 1.0)).collect();
        let period = bill_period(&days, &BTreeMap::new(), at(1, 0, 0), at(11, 0, 0), Some(2));
        assert_eq!(period.days_total, 30);
        assert_eq!(period.days_with_data, 10);
        assert!(close(period.so_far.grid_kwh, 20.0));
        let projected = period.projected.unwrap();
        assert!(close(projected.grid_kwh, 60.0));
        assert!(close(projected.meter1_kwh, 10.0));
        assert!(close(projected.meter2_kwh, 50.0));
    }

    #[test]
    fn reading_days_count_only_their_side_of_the_reading_time() {
        let august = NaiveDate::from_ymd_opt(2026, 8, 23).unwrap();
        let days = vec![
            day_units(august, 4.0, 0.0),
            day_units(day(1), 1.0, 0.0),
            day_units(day(23), 0.0, 2.0),
            day_units(day(24), 9.0, 9.0),
        ];
        let mut splits = BTreeMap::new();
        let mut before = DayUnits::empty(day(23));
        before.grid.add(Some(2), 0.5);
        before.data_hours = 12.0;
        splits.insert(day(23), DaySplit { before, after: DayUnits::empty(day(23)) });
        let period = bill_period(&days, &splits, on(8, 23, 12, 0), on(9, 28, 9, 0), None);
        assert_eq!(period.end, on(9, 23, 12, 0));
        assert_eq!(period.days_total, 31);
        assert!(close(period.so_far.meter1_kwh, 2.0 + 1.0));
        assert!(close(period.so_far.meter2_kwh, 0.5));
        assert!(close(period.log_hours, 12.0));
        assert!(close(period.elapsed_hours, 31.0 * 24.0));
        assert!(period.projected.is_none());
    }

    #[test]
    fn standby_is_added_for_grid_on_hours_in_the_days_meter_shares() {
        let mut units = day_units(day(24), 3.0, 1.0);
        units.data_hours = 24.0;
        units.grid_off_minutes = 240.0;
        let corrected = units.with_standby(10.0);
        assert!(close(corrected.standby_kwh, 0.2));
        assert!(close(corrected.grid.grid_kwh, 4.2));
        assert!(close(corrected.grid.meter1_kwh, 3.15));
        assert!(close(corrected.grid.meter2_kwh, 1.05));
        let mut quiet = DayUnits::empty(day(25));
        quiet.data_hours = 10.0;
        let quiet = quiet.with_standby(10.0);
        assert!(close(quiet.grid.unassigned_kwh, 0.1));
    }

    #[test]
    fn split_day_cuts_at_the_reading_minute() {
        let points: Vec<UnitPoint> = (0..24).flat_map(|hour| hour_of(23, hour, 600.0, 0.0, 0.0, true)).collect();
        let switches = [MeterSwitch { at: at(23, 0, 0), meter: 2 }];
        let split = split_day(&points, &switches, at(23, 11, 57), at(28, 0, 0));
        assert!(close(split.before.grid.meter2_kwh, 0.6 * (11.0 + 57.0 / 60.0)));
        assert!(close(split.after.grid.meter2_kwh, 0.6 * (12.0 + 3.0 / 60.0)));
        let today = split_day(&points, &switches, at(23, 11, 57), at(23, 13, 0));
        assert!(close(today.after.grid.meter2_kwh, 0.6 * (63.0 / 60.0)));
    }

    #[test]
    fn history_points_convert_to_local_time_and_watts() {
        use std::collections::BTreeMap;
        let mut values = BTreeMap::new();
        values.insert("load_power".to_string(), Some(0.24));
        values.insert("pvInputPower".to_string(), Some(0.0));
        values.insert("batteryDischargeCurrent".to_string(), Some(13.0));
        values.insert("batteryVoltage".to_string(), Some(26.0));
        values.insert("acInputVoltage".to_string(), Some(238.7));
        let raw = HistoryPoint { at: "2026-09-20T19:09:59.932Z".parse().unwrap(), values };
        let mut no_voltage = raw.clone();
        no_voltage.values.insert("acInputVoltage".to_string(), None);
        let points = unit_points(&[raw, no_voltage], chrono_tz::Asia::Karachi);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].at, at(21, 0, 9) + Duration::seconds(59));
        assert!(close(points[0].house_w, 240.0));
        assert!(close(points[0].discharge_w, 338.0));
        assert!(points[0].grid_on);
    }

    fn unassigned_day(kwh: f64) -> DayUnits {
        let mut units = DayUnits::empty(day(26));
        units.grid.grid_kwh = kwh;
        units.grid.unassigned_kwh = kwh;
        units
    }

    #[test]
    fn assigning_a_whole_stretch_moves_all_its_unassigned_units() {
        let window = (at(26, 0, 0), at(27, 0, 0));
        let assignment = MeterAssignment { from: at(25, 12, 0), to: at(27, 6, 0), meter: 2 };
        let units = unassigned_day(1.2).with_assignments(window, None, &[assignment]);
        assert!(close(units.grid.meter2_kwh, 1.2));
        assert!(close(units.grid.unassigned_kwh, 0.0));
        assert!(close(units.grid.grid_kwh, 1.2), "the total never changes");
    }

    #[test]
    fn a_partial_stretch_moves_its_share_and_only_the_time_before_the_first_switch_counts() {
        let window = (at(26, 0, 0), at(27, 0, 0));
        let first_half = MeterAssignment { from: at(26, 0, 0), to: at(26, 6, 0), meter: 1 };
        let units = unassigned_day(1.0).with_assignments(window, Some(at(26, 12, 0)), &[first_half]);
        assert!(close(units.grid.meter1_kwh, 0.5), "6 of the 12 unassigned hours");
        assert!(close(units.grid.unassigned_kwh, 0.5));

        let after_the_switch = MeterAssignment { from: at(26, 13, 0), to: at(26, 20, 0), meter: 1 };
        let untouched = unassigned_day(1.0).with_assignments(window, Some(at(26, 12, 0)), &[after_the_switch]);
        assert!(close(untouched.grid.unassigned_kwh, 1.0));
    }

    #[test]
    fn overlapping_assignments_are_detected() {
        let a = MeterAssignment { from: at(25, 0, 0), to: at(26, 0, 0), meter: 1 };
        let b = MeterAssignment { from: at(25, 12, 0), to: at(27, 0, 0), meter: 2 };
        let c = MeterAssignment { from: at(26, 0, 0), to: at(27, 0, 0), meter: 2 };
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c), "touching ends don't overlap");
    }
}
