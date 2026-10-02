use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use chrono::{Datelike, Months, NaiveDate, NaiveDateTime};

use super::units::{DayUnits, GridSplit, MeterAssignment, MeterSwitch};
use crate::automation::history::{num, opt_f64, split_csv_line, HistoryStore};

const KIND: &str = "energy";
const DAY_HEADER: &str =
    "date,grid_units,meter1_units,meter2_units,unassigned_units,house_kwh,solar_kwh,battery_kwh,grid_off_minutes,data_hours";
const SWITCH_HEADER: &str = "timestamp,meter";
const SWITCHES_FILE: &str = "meter-switches.csv";
const ASSIGNMENT_HEADER: &str = "from,to,meter";
const ASSIGNMENTS_FILE: &str = "meter-assignments.csv";
const DATE_FORMAT: &str = "%Y-%m-%d";
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Daily unit totals (`energy/YYYY-MM.csv`, one row per finished day) and the
/// changeover-switch log (`energy/meter-switches.csv`). Append-only.
#[derive(Debug, Clone)]
pub struct EnergyRecords {
    store: HistoryStore,
}

impl EnergyRecords {
    pub fn new(store: HistoryStore) -> Self {
        Self { store }
    }

    pub fn append_day(&self, day: &DayUnits) -> std::io::Result<()> {
        self.store.append(KIND, DAY_HEADER, day.date, &day_to_csv(day))
    }

    /// Saved days from `from` to `to` inclusive; a date written twice keeps its
    /// last row.
    pub fn days_between(&self, from: NaiveDate, to: NaiveDate) -> Vec<DayUnits> {
        let mut days = BTreeMap::new();
        let mut month = from.with_day(1).unwrap_or(from);
        while month <= to {
            for day in self.store.read_lines(KIND, month).iter().filter_map(|line| parse_day(line)) {
                if day.date >= from && day.date <= to {
                    days.insert(day.date, day);
                }
            }
            let Some(next) = month.checked_add_months(Months::new(1)) else { break };
            month = next;
        }
        days.into_values().collect()
    }

    fn switches_path(&self) -> PathBuf {
        self.store.root().join(KIND).join(SWITCHES_FILE)
    }

    pub fn meter_switches(&self) -> Vec<MeterSwitch> {
        let mut switches: Vec<MeterSwitch> = fs::read_to_string(self.switches_path())
            .map(|text| text.lines().skip(1).filter_map(parse_switch).collect())
            .unwrap_or_default();
        switches.sort_by_key(|switch| switch.at);
        switches
    }

    fn assignments_path(&self) -> PathBuf {
        self.store.root().join(KIND).join(ASSIGNMENTS_FILE)
    }

    pub fn meter_assignments(&self) -> Vec<MeterAssignment> {
        let mut assignments: Vec<MeterAssignment> = fs::read_to_string(self.assignments_path())
            .map(|text| text.lines().skip(1).filter_map(parse_assignment).collect())
            .unwrap_or_default();
        assignments.sort_by_key(|assignment| assignment.from);
        assignments
    }

    fn write_assignments(&self, assignments: &[MeterAssignment]) -> std::io::Result<()> {
        let path = self.assignments_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = format!("{ASSIGNMENT_HEADER}\n");
        for assignment in assignments {
            text.push_str(&format!(
                "{},{},{}\n",
                assignment.from.format(TIMESTAMP_FORMAT),
                assignment.to.format(TIMESTAMP_FORMAT),
                assignment.meter
            ));
        }
        fs::write(path, text)
    }

    pub fn add_assignment(&self, assignment: MeterAssignment) -> Result<(), String> {
        let mut assignments = self.meter_assignments();
        if assignments.iter().any(|existing| existing.overlaps(&assignment)) {
            return Err("That overlaps a period you already assigned. Remove it first.".into());
        }
        assignments.push(assignment);
        assignments.sort_by_key(|assignment| assignment.from);
        self.write_assignments(&assignments).map_err(|error| format!("Couldn't save the assignment: {error}"))
    }

    pub fn remove_assignment(&self, from: NaiveDateTime) -> Result<(), String> {
        let mut assignments = self.meter_assignments();
        assignments.retain(|assignment| assignment.from != from);
        self.write_assignments(&assignments).map_err(|error| format!("Couldn't save the change: {error}"))
    }

    pub fn append_switch(&self, switch: MeterSwitch) -> std::io::Result<()> {
        let path = self.switches_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let is_new = !path.exists();
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        if is_new {
            writeln!(file, "{SWITCH_HEADER}")?;
        }
        writeln!(file, "{},{}", switch.at.format(TIMESTAMP_FORMAT), switch.meter)
    }
}

fn day_to_csv(day: &DayUnits) -> String {
    [
        day.date.format(DATE_FORMAT).to_string(),
        num(Some(day.grid.grid_kwh), 3),
        num(Some(day.grid.meter1_kwh), 3),
        num(Some(day.grid.meter2_kwh), 3),
        num(Some(day.grid.unassigned_kwh), 3),
        num(Some(day.house_kwh), 3),
        num(Some(day.solar_kwh), 3),
        num(Some(day.battery_kwh), 3),
        num(Some(day.grid_off_minutes), 0),
        num(Some(day.data_hours), 2),
    ]
    .join(",")
}

fn parse_day(line: &str) -> Option<DayUnits> {
    let f = split_csv_line(line);
    if f.len() < 10 {
        return None;
    }
    let value = |index: usize| opt_f64(&f[index]).unwrap_or(0.0);
    Some(DayUnits {
        date: NaiveDate::parse_from_str(f[0].trim(), DATE_FORMAT).ok()?,
        grid: GridSplit {
            grid_kwh: value(1),
            meter1_kwh: value(2),
            meter2_kwh: value(3),
            unassigned_kwh: value(4),
        },
        house_kwh: value(5),
        solar_kwh: value(6),
        battery_kwh: value(7),
        grid_off_minutes: value(8),
        data_hours: value(9),
        standby_kwh: 0.0,
    })
}

fn parse_assignment(line: &str) -> Option<MeterAssignment> {
    let mut fields = line.split(',');
    let (from, to, meter) = (fields.next()?, fields.next()?, fields.next()?);
    let assignment = MeterAssignment {
        from: NaiveDateTime::parse_from_str(from.trim(), TIMESTAMP_FORMAT).ok()?,
        to: NaiveDateTime::parse_from_str(to.trim(), TIMESTAMP_FORMAT).ok()?,
        meter: meter.trim().parse().ok().filter(|meter| matches!(meter, 1 | 2))?,
    };
    (assignment.from < assignment.to).then_some(assignment)
}

fn parse_switch(line: &str) -> Option<MeterSwitch> {
    let (timestamp, meter) = line.split_once(',')?;
    let meter: u8 = meter.trim().parse().ok().filter(|meter| matches!(meter, 1 | 2))?;
    Some(MeterSwitch {
        at: NaiveDateTime::parse_from_str(timestamp.trim(), TIMESTAMP_FORMAT).ok()?,
        meter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(month: u32, date: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, date).unwrap()
    }

    fn records() -> (EnergyRecords, PathBuf) {
        let dir = std::env::temp_dir().join(format!("solar-hub-energy-{}", uuid::Uuid::new_v4()));
        (EnergyRecords::new(HistoryStore::new(dir.clone())), dir)
    }

    fn units(date: NaiveDate, grid: f64) -> DayUnits {
        let mut units = DayUnits::empty(date);
        units.grid.grid_kwh = grid;
        units.grid.meter2_kwh = grid;
        units.house_kwh = 7.61;
        units.solar_kwh = 4.87;
        units.battery_kwh = 1.99;
        units.grid_off_minutes = 17.0;
        units.data_hours = 24.0;
        units
    }

    #[test]
    fn days_round_trip_across_months_and_dedupe() {
        let (records, dir) = records();
        records.append_day(&units(day(8, 31), 4.2)).unwrap();
        records.append_day(&units(day(9, 27), 0.75)).unwrap();
        records.append_day(&units(day(9, 27), 0.8)).unwrap();
        records.append_day(&units(day(10, 1), 1.0)).unwrap();
        let text = fs::read_to_string(dir.join("energy").join("2026-09.csv")).unwrap();
        assert!(text.starts_with(DAY_HEADER));
        let days = records.days_between(day(8, 31), day(9, 30));
        assert_eq!(days.len(), 2);
        assert_eq!(days[1], units(day(9, 27), 0.8));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn switches_round_trip_sorted_and_skip_bad_rows() {
        let (records, dir) = records();
        let later = MeterSwitch { at: day(9, 28).and_hms_opt(14, 5, 0).unwrap(), meter: 2 };
        let earlier = MeterSwitch { at: day(9, 28).and_hms_opt(9, 0, 0).unwrap(), meter: 1 };
        records.append_switch(later).unwrap();
        records.append_switch(earlier).unwrap();
        let mut file = OpenOptions::new().append(true).open(records.switches_path()).unwrap();
        writeln!(file, "2026-09-28 15:00:00,3").unwrap();
        assert_eq!(records.meter_switches(), vec![earlier, later]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn assignments_round_trip_reject_overlaps_and_can_be_removed() {
        let (records, dir) = records();
        let first = MeterAssignment { from: day(9, 23).and_hms_opt(11, 57, 0).unwrap(), to: day(9, 28).and_hms_opt(18, 30, 0).unwrap(), meter: 2 };
        let overlapping = MeterAssignment { from: day(9, 25).and_hms_opt(0, 0, 0).unwrap(), to: day(9, 26).and_hms_opt(0, 0, 0).unwrap(), meter: 1 };
        let later = MeterAssignment { from: day(9, 29).and_hms_opt(0, 0, 0).unwrap(), to: day(9, 29).and_hms_opt(6, 0, 0).unwrap(), meter: 1 };
        records.add_assignment(later).unwrap();
        records.add_assignment(first).unwrap();
        assert!(records.add_assignment(overlapping).is_err());
        assert_eq!(records.meter_assignments(), vec![first, later]);
        records.remove_assignment(first.from).unwrap();
        assert_eq!(records.meter_assignments(), vec![later]);
        fs::remove_dir_all(dir).unwrap();
    }
}
