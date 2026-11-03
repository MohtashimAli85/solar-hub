use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;

use crate::automation::agent::{EvDayRow, EvHourRow, EvInput};
use crate::automation::config::AutomationConfig;
use crate::automation::history::Sample;
use crate::automation::telemetry::{soc_per_ah, FULL_SOC};
use crate::automation::weather::{Forecast, PERFORMANCE_RATIO};

pub const GEYSER_TITLE: &str = "Geyser";
pub const EV_TITLE: &str = "Scooter";

const STEP_MINUTES: i64 = 15;
/// The battery has to reach tonight's need plus this by sunset, as cover for
/// a forecast that turns out a little brighter than the day.
const SUNSET_MARGIN_SOC: f64 = 5.0;
const FLOOR_MARGIN_SOC: f64 = 5.0;
const MIN_RUN_MINUTES: i64 = 30;
const CHARGER_EFFICIENCY: f64 = 0.9;
const INVERTER_EFFICIENCY: f64 = 0.9;
/// Load steps smaller than this are fans and fridges, not one of the loads.
const MIN_STEP_W: f64 = 400.0;
const MAX_GAP_MINUTES: i64 = 20;
const LOST_MARGIN_W: f64 = 250.0;
/// A load the app just asked for wins a close match against one it didn't.
const EXPECTED_BONUS_W: f64 = 100.0;
const EXPECT_MINUTES: i64 = 20;
const STOP_NOTIFY_AFTER_MINUTES: i64 = 10;
const READY_AFTER_STOP_MINUTES: i64 = 20;
/// A charger that switches itself off above this estimate has filled the pack.
const EV_FULL_GUESS_SOC: f64 = 80.0;
const EV_READ_MINUTES: i64 = 30;
const EV_NIGHT_READ_MINUTES: i64 = 120;
const EV_LIVE_READ_MINUTES: i64 = 60;
const EV_ASK_SOC_CHANGE: f64 = 15.0;
const ACTIVE_TICK_MINUTES: i64 = 2;
const IDLE_TICK_MINUTES: i64 = 5;
const RATIO_MIN_SAMPLES: usize = 12;
const RATIO_RANGE: (f64, f64) = (0.35, 1.0);
const RATIO_MIN_RADIATION: f64 = 300.0;
const RATIO_PERCENTILE: f64 = 0.75;
const GEYSER_COMMAND_WAIT_MINUTES: i64 = 3;
const GEYSER_COMMAND_GAP_MINUTES: i64 = 5;
const GEYSER_TOP_UP_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadKind {
    Water,
    Geyser,
    Ev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoadPhase {
    #[default]
    Off,
    Waiting,
    Ready,
    Running,
    Stop,
    Done,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SocBasis {
    Read,
    Live,
    Estimated,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvCharge {
    Yes,
    IfSpare,
    No,
}

impl EvCharge {
    pub fn parse(text: &str) -> EvCharge {
        match text.trim().to_ascii_lowercase().as_str() {
            "yes" => EvCharge::Yes,
            "no" => EvCharge::No,
            _ => EvCharge::IfSpare,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slot {
    pub kind: LoadKind,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub watts: f64,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct LoadRow {
    pub phase: LoadPhase,
    pub note: Option<String>,
    pub watts: Option<f64>,
    pub from: Option<NaiveDateTime>,
    pub until: Option<NaiveDateTime>,
    pub running_w: Option<f64>,
    pub running_since: Option<NaiveDateTime>,
    pub last_reminded_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct GeyserStatus {
    #[serde(flatten)]
    pub row: LoadRow,
    pub delivered_kwh: f64,
    pub need_kwh: f64,
    pub heated_at: Option<NaiveDateTime>,
    pub auto: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct EvStatus {
    #[serde(flatten)]
    pub row: LoadRow,
    pub soc: Option<f64>,
    pub soc_at: Option<NaiveDateTime>,
    pub soc_basis: Option<SocBasis>,
    pub hours_to_full: Option<f64>,
    pub decision: Option<EvCharge>,
    pub decision_reason: Option<String>,
    pub decision_model: Option<String>,
    pub reading: bool,
    pub read_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct LoadsStatus {
    pub geyser: GeyserStatus,
    pub ev: EvStatus,
    pub water_running_since: Option<NaiveDateTime>,
    pub spare_kwh: Option<f64>,
    pub sunset_target_soc: Option<f64>,
    pub projected_sunset_soc: Option<f64>,
    pub pv_ratio: Option<f64>,
    pub plan: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvReading {
    pub soc: f64,
    pub at: NaiveDateTime,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvDecision {
    pub charge: EvCharge,
    pub reason: String,
    pub model: Option<String>,
    pub at: NaiveDateTime,
    pub soc: f64,
    pub recheck_at: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeyserIntent {
    On(f64),
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeyserCommand {
    pub intent: GeyserIntent,
    /// The remote button to press: "power" or the level ("w800", …).
    pub press_power: bool,
}

pub struct LoadsInput<'a> {
    pub now: NaiveDateTime,
    pub daytime: bool,
    pub sunset: NaiveDateTime,
    pub soc: Option<f64>,
    pub rated_ah: f64,
    pub volts: f64,
    pub max_charge_a: Option<f64>,
    pub charge_a: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub forecast: Option<&'a Forecast>,
    pub pv_ratio: f64,
    pub typical_load: &'a dyn Fn(u32) -> Option<f64>,
    pub tonight_needs_soc: Option<f64>,
    pub config: &'a AutomationConfig,
    pub water_pending: bool,
    pub geyser_done_on: Option<NaiveDate>,
    pub ev_skip_on: Option<NaiveDate>,
    pub ev_reading: Option<EvReading>,
    pub ev_reader_busy: bool,
    pub remote_ready: bool,
}

#[derive(Debug, Default)]
pub struct LoadsStep {
    pub status: LoadsStatus,
    pub notify: Vec<(&'static str, String)>,
    pub geyser_command: Option<GeyserCommand>,
    /// Some(power_cycle) asks for a scooter read.
    pub ev_read: Option<bool>,
    pub ev_input: Option<EvInput>,
    pub next_at: Option<NaiveDateTime>,
}

pub fn geyser_levels(config: &AutomationConfig) -> Vec<f64> {
    let (low, high) = (config.geyser_low_watts, config.geyser_high_watts);
    let mut levels = vec![low + high, high.max(low), high.min(low)];
    levels.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    levels
}

pub fn ev_rate_pct_per_hour(config: &AutomationConfig) -> f64 {
    config.ev_charger_watts * CHARGER_EFFICIENCY / (config.ev_battery_kwh * 1000.0) * 100.0
}

pub fn ev_need_wh(soc: f64, config: &AutomationConfig) -> f64 {
    (100.0 - soc).max(0.0) / 100.0 * config.ev_battery_kwh * 1000.0 / CHARGER_EFFICIENCY
}

fn clock_time(at: NaiveDateTime) -> String {
    at.format("%-I:%M %p").to_string()
}

fn duration_text(hours: f64) -> String {
    let minutes = (hours * 60.0).round().max(0.0) as i64;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn at_hour(date: NaiveDate, hour: u32) -> NaiveDateTime {
    date.and_hms_opt(hour.min(23), 0, 0).unwrap_or_else(|| date.and_time(chrono::NaiveTime::MIN))
}

/// How the panels compare with their nameplate under the forecast's
/// radiation: only samples where the battery was still taking charge below
/// its limit count, since a full or capped battery holds the panels down.
pub fn pv_ratio(samples: &[Sample], forecast: &Forecast, pv_array_watts: f64, max_charge_a: Option<f64>, since: NaiveDateTime) -> Option<f64> {
    if pv_array_watts <= 0.0 {
        return None;
    }
    let charge_cap = max_charge_a.map_or(30.0, |max| max * 0.8);
    let mut ratios: Vec<f64> = samples
        .iter()
        .filter(|s| s.at >= since && s.soc.is_some_and(|soc| soc < 95.0))
        .filter(|s| s.battery_a.is_some_and(|amps| (1.0..charge_cap).contains(&amps)))
        .filter_map(|s| {
            let pv = s.pv_w.filter(|pv| *pv >= 300.0)?;
            let radiation = forecast.radiation_during(s.at).filter(|r| *r >= RATIO_MIN_RADIATION)?;
            Some(pv / (radiation * pv_array_watts / 1000.0))
        })
        .collect();
    if ratios.len() < RATIO_MIN_SAMPLES {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    let index = ((ratios.len() - 1) as f64 * RATIO_PERCENTILE).round() as usize;
    Some(ratios[index].clamp(RATIO_RANGE.0, RATIO_RANGE.1))
}

pub fn default_pv_ratio() -> f64 {
    PERFORMANCE_RATIO
}

#[derive(Debug, Clone, Copy)]
struct Step {
    start: NaiveDateTime,
    hours: f64,
    surplus_w: f64,
}

impl Step {
    fn end(&self) -> NaiveDateTime {
        self.start + Duration::seconds((self.hours * 3600.0).round() as i64)
    }
}

struct Sun<'a> {
    now: NaiveDateTime,
    sunset: NaiveDateTime,
    soc: f64,
    rated_ah: f64,
    volts: f64,
    max_charge_a: Option<f64>,
    floor: f64,
    sunset_target: f64,
    pv_now_w: Option<f64>,
    house_now_w: Option<f64>,
    pv_at: &'a dyn Fn(NaiveDateTime) -> Option<f64>,
    house_at: &'a dyn Fn(u32) -> Option<f64>,
}

fn steps(sun: &Sun) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    let mut at = sun.now;
    while at < sun.sunset {
        let hour_start = at.date().and_hms_opt(at.hour(), 0, 0).unwrap_or(at);
        let boundary = hour_start + Duration::minutes((at.minute() as i64 / STEP_MINUTES + 1) * STEP_MINUTES);
        let end = boundary.min(sun.sunset);
        let first = out.is_empty();
        let pv = if first { sun.pv_now_w.or_else(|| (sun.pv_at)(at)) } else { (sun.pv_at)(at) };
        let house = if first {
            sun.house_now_w.or_else(|| (sun.house_at)(at.hour()))
        } else {
            (sun.house_at)(at.hour()).or(sun.house_now_w)
        };
        out.push(Step {
            start: at,
            hours: (end - at).num_seconds() as f64 / 3600.0,
            surplus_w: pv.unwrap_or(0.0) - house.unwrap_or(0.0),
        });
        at = end;
    }
    out
}

struct Run {
    end_soc: f64,
    low_soc: f64,
    curtailed_wh: f64,
}

fn simulate(sun: &Sun, steps: &[Step], extra: &[f64]) -> Run {
    let per_ah = soc_per_ah(sun.rated_ah);
    let mut soc = sun.soc;
    let mut low = soc;
    let mut curtailed_wh = 0.0;
    for (step, extra) in steps.iter().zip(extra) {
        let net = step.surplus_w - extra;
        if net >= 0.0 {
            let amps = net / sun.volts;
            let taken = sun.max_charge_a.map_or(amps, |max| amps.min(max));
            let room_ah = (100.0 - soc).max(0.0) / per_ah.max(f64::EPSILON);
            let charged_ah = (taken * step.hours).min(room_ah);
            curtailed_wh += (net * step.hours - charged_ah * sun.volts).max(0.0);
            soc = (soc + charged_ah * per_ah).min(100.0);
        } else {
            soc = (soc + net / sun.volts / INVERTER_EFFICIENCY * step.hours * per_ah).max(0.0);
        }
        low = low.min(soc);
    }
    Run { end_soc: soc, low_soc: low, curtailed_wh }
}

struct Limits {
    target: f64,
    low: f64,
}

fn fits(sun: &Sun, steps: &[Step], extra: &[f64], limits: &Limits) -> bool {
    let run = simulate(sun, steps, extra);
    run.end_soc >= limits.target - 1e-6 && run.low_soc >= limits.low - 1e-6
}

#[derive(Debug, Clone)]
pub struct Need {
    pub kind: LoadKind,
    pub levels: Vec<f64>,
    pub need_wh: f64,
    pub from: NaiveDateTime,
    pub until: NaiveDateTime,
    pub running_w: Option<f64>,
    pub partial: bool,
}

struct Candidate {
    start: usize,
    end: usize,
    watts: f64,
    delivered_wh: f64,
    battery_wh: f64,
}

fn better(a: &Candidate, b: &Candidate) -> bool {
    let tolerance = 1.0;
    if (a.delivered_wh - b.delivered_wh).abs() > tolerance {
        return a.delivered_wh > b.delivered_wh;
    }
    if (a.battery_wh - b.battery_wh).abs() > tolerance {
        return a.battery_wh < b.battery_wh;
    }
    if a.start != b.start {
        return a.start < b.start;
    }
    a.watts > b.watts
}

/// The best contiguous run for one load: as much of its need as fits, with
/// the least battery, the battery still reaching tonight's need by sunset.
fn place(sun: &Sun, steps: &[Step], extra: &mut [f64], limits: &Limits, need: &Need) -> Option<Slot> {
    if need.need_wh <= 0.0 || need.levels.is_empty() {
        return None;
    }
    let inside = |i: usize| steps[i].start < need.until && steps[i].end() > need.from;
    let starts: Vec<usize> = match need.running_w {
        Some(_) => (0..steps.len().min(1)).filter(|i| inside(*i)).collect(),
        None => (0..steps.len()).filter(|i| inside(*i)).collect(),
    };
    let levels: Vec<f64> = match need.running_w {
        Some(running) => need.levels.iter().copied().filter(|w| *w <= running + 1.0).collect(),
        None => need.levels.clone(),
    };
    let min_hours = MIN_RUN_MINUTES as f64 / 60.0;
    let mut best: Option<Candidate> = None;
    let mut trial = extra.to_vec();
    for &start in &starts {
        for &watts in &levels {
            let mut energy = 0.0;
            let mut hours = 0.0;
            let mut battery_wh = 0.0;
            let mut end = start;
            while end < steps.len() && inside(end) {
                let step = steps[end];
                energy += watts * step.hours;
                hours += step.hours;
                battery_wh += (watts - (step.surplus_w - extra[end]).max(0.0)).max(0.0) * step.hours;
                let full = energy >= need.need_wh - 1.0;
                let long_enough = hours >= min_hours - 1e-6 || need.running_w.is_some();
                if full || (need.partial && long_enough) {
                    for i in start..=end {
                        trial[i] = extra[i] + watts;
                    }
                    let candidate = Candidate { start, end, watts, delivered_wh: energy.min(need.need_wh), battery_wh };
                    if fits(sun, steps, &trial, limits) && best.as_ref().is_none_or(|b| better(&candidate, b)) {
                        best = Some(candidate);
                    }
                    trial[start..=end].copy_from_slice(&extra[start..=end]);
                }
                if full {
                    break;
                }
                end += 1;
            }
        }
    }
    let best = best?;
    for value in &mut extra[best.start..=best.end] {
        *value += best.watts;
    }
    Some(Slot {
        kind: need.kind,
        start: steps[best.start].start,
        end: steps[best.end].end(),
        watts: best.watts,
        partial: best.delivered_wh < need.need_wh - 1.0,
    })
}

struct Plan {
    slots: Vec<Slot>,
    end_soc: f64,
    spare_wh: f64,
    spare_before_ev_wh: f64,
    hours: Vec<(NaiveDateTime, f64)>,
}

fn plan(sun: &Sun, needs: &[Need]) -> Option<Plan> {
    if sun.volts <= 0.0 || sun.rated_ah <= 0.0 {
        return None;
    }
    let steps = steps(sun);
    if steps.is_empty() {
        return None;
    }
    let mut extra = vec![0.0; steps.len()];
    let baseline = simulate(sun, &steps, &extra);
    let limits = Limits { target: sun.sunset_target, low: (sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc) };
    let cap_wh = sun.rated_ah * sun.volts;
    let spare = |extra: &[f64]| {
        let run = simulate(sun, &steps, extra);
        (run.curtailed_wh + (run.end_soc - limits.target) / 100.0 * cap_wh).max(0.0)
    };
    let mut slots = Vec::new();
    let mut spare_before_ev_wh = None;
    for need in needs {
        if need.kind == LoadKind::Ev && need.need_wh > 0.0 && spare_before_ev_wh.is_none() {
            spare_before_ev_wh = Some(spare(&extra));
        }
        if let Some(slot) = place(sun, &steps, &mut extra, &limits, need) {
            slots.push(slot);
        }
    }
    let hours = steps
        .iter()
        .filter(|step| step.start.minute() == 0 || step.start == sun.now)
        .map(|step| (step.start, step.surplus_w))
        .collect();
    Some(Plan {
        end_soc: simulate(sun, &steps, &extra).end_soc,
        spare_wh: spare(&extra),
        spare_before_ev_wh: spare_before_ev_wh.unwrap_or_else(|| spare(&extra)),
        hours,
        slots,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Running {
    pub watts: f64,
    pub since: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Started(LoadKind, f64),
    Stopped(LoadKind, f64),
    Level(LoadKind, f64, f64),
    Lost(LoadKind),
}

impl Change {
    fn kind(&self) -> LoadKind {
        match self {
            Change::Started(kind, _) | Change::Stopped(kind, _) | Change::Level(kind, _, _) | Change::Lost(kind) => *kind,
        }
    }
}

/// Works out which load switched on or off from steps in the house load.
#[derive(Debug, Default)]
pub struct Detector {
    last: Option<(NaiveDateTime, f64)>,
    running: BTreeMap<LoadKind, Running>,
}

fn tolerance(step: f64) -> f64 {
    (step.abs() * 0.2).max(150.0)
}

impl Detector {
    pub fn running(&self, kind: LoadKind) -> Option<&Running> {
        self.running.get(&kind)
    }

    fn running_w(&self) -> f64 {
        self.running.values().map(|r| r.watts).sum()
    }

    pub fn observe(&mut self, now: NaiveDateTime, load_w: f64, options: &[(LoadKind, Vec<f64>)], expected: &[LoadKind]) -> Vec<Change> {
        self.running.retain(|kind, _| options.iter().any(|(option, _)| option == kind));
        let previous = self.last.replace((now, load_w));
        let mut changes = Vec::new();
        if let Some((at, before)) = previous {
            let delta = load_w - before;
            if now - at <= Duration::minutes(MAX_GAP_MINUTES) && delta.abs() >= MIN_STEP_W {
                if let Some(change) = self.best_match(delta, options, expected) {
                    self.apply(&change, now);
                    changes.push(change);
                }
            }
        }
        while self.running_w() > load_w + LOST_MARGIN_W {
            let Some(kind) = self.running.iter().max_by_key(|(_, r)| r.since).map(|(kind, _)| *kind) else {
                break;
            };
            self.running.remove(&kind);
            changes.push(Change::Lost(kind));
        }
        changes
    }

    fn best_match(&self, delta: f64, options: &[(LoadKind, Vec<f64>)], expected: &[LoadKind]) -> Option<Change> {
        let mut candidates: Vec<(Change, f64)> = Vec::new();
        for (kind, levels) in options {
            match self.running.get(kind) {
                Some(running) => {
                    candidates.push((Change::Stopped(*kind, running.watts), -running.watts));
                    for level in levels.iter().filter(|level| (*level - running.watts).abs() > 1.0) {
                        candidates.push((Change::Level(*kind, running.watts, *level), level - running.watts));
                    }
                }
                None => candidates.extend(levels.iter().map(|level| (Change::Started(*kind, *level), *level))),
            }
        }
        candidates
            .into_iter()
            .filter(|(_, step)| step.signum() == delta.signum() && (delta - step).abs() <= tolerance(*step))
            .map(|(change, step)| {
                let bonus = if expected.contains(&change.kind()) { EXPECTED_BONUS_W } else { 0.0 };
                ((delta - step).abs() - bonus, change)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, change)| change)
    }

    fn apply(&mut self, change: &Change, now: NaiveDateTime) {
        match *change {
            Change::Started(kind, watts) => {
                self.running.insert(kind, Running { watts, since: now });
            }
            Change::Stopped(kind, _) | Change::Lost(kind) => {
                self.running.remove(&kind);
            }
            Change::Level(kind, _, to) => {
                if let Some(running) = self.running.get_mut(&kind) {
                    running.watts = to;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct RowMemory {
    phase: LoadPhase,
    ready_at: Option<NaiveDateTime>,
    last_reminded_at: Option<NaiveDateTime>,
    stop_since: Option<NaiveDateTime>,
    last_stop_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
struct EvSoc {
    soc: f64,
    at: NaiveDateTime,
    basis: SocBasis,
}

#[derive(Debug, Default)]
struct EvMemory {
    soc: Option<EvSoc>,
    last_reading_at: Option<NaiveDateTime>,
    last_passive_soc: Option<f64>,
    last_read_request: Option<NaiveDateTime>,
    last_live_request: Option<NaiveDateTime>,
    decision: Option<EvDecision>,
    ask_after: Option<NaiveDateTime>,
}

#[derive(Debug, Default)]
struct GeyserControl {
    pending: Option<(GeyserIntent, NaiveDateTime, u8)>,
    last_sent_at: Option<NaiveDateTime>,
    failed_on: Option<NaiveDate>,
}

#[derive(Debug, Default)]
pub struct LoadsMemory {
    day: Option<NaiveDate>,
    detector: Detector,
    rows: BTreeMap<LoadKind, RowMemory>,
    last_at: Option<NaiveDateTime>,
    geyser_delivered_wh: f64,
    geyser_heated_at: Option<NaiveDateTime>,
    geyser_weak_notified: bool,
    geyser: GeyserControl,
    ev: EvMemory,
}

impl LoadsMemory {
    pub fn record_ev_decision(&mut self, decision: EvDecision) {
        self.ev.decision = Some(decision);
        self.ev.ask_after = None;
    }

    pub fn ev_ask_failed(&mut self, retry_at: NaiveDateTime) {
        self.ev.ask_after = Some(retry_at);
    }

    pub fn geyser_sent(&mut self, command: GeyserCommand, now: NaiveDateTime, ok: bool) {
        self.geyser.last_sent_at = Some(now);
        if ok {
            let attempts = match self.geyser.pending {
                Some((intent, _, attempts)) if intent == command.intent => attempts + 1,
                _ => 1,
            };
            self.geyser.pending = Some((command.intent, now, attempts));
        } else {
            self.geyser.failed_on = Some(now.date());
            self.geyser.pending = None;
        }
    }

    fn new_day(&mut self, today: NaiveDate) {
        if self.day == Some(today) {
            return;
        }
        self.day = Some(today);
        self.rows.clear();
        self.geyser_delivered_wh = 0.0;
        self.geyser_heated_at = None;
        self.geyser_weak_notified = false;
        self.geyser = GeyserControl::default();
    }

    fn expected(&self, now: NaiveDateTime) -> Vec<LoadKind> {
        let mut kinds: Vec<LoadKind> = self
            .rows
            .iter()
            .filter(|(_, row)| row.ready_at.is_some_and(|at| now - at <= Duration::minutes(EXPECT_MINUTES)))
            .map(|(kind, _)| *kind)
            .collect();
        if self.geyser.pending.is_some() {
            kinds.push(LoadKind::Geyser);
        }
        kinds
    }

    fn update_ev_soc(&mut self, reading: Option<&EvReading>, charging: bool, now: NaiveDateTime, config: &AutomationConfig) {
        if let Some(reading) = reading.filter(|r| self.ev.last_reading_at.is_none_or(|at| r.at > at)) {
            self.ev.last_reading_at = Some(reading.at);
            let news = reading.live || self.ev.soc.is_none() || self.ev.last_passive_soc != Some(reading.soc);
            if !reading.live {
                self.ev.last_passive_soc = Some(reading.soc);
            }
            if news && (reading.live || !charging || self.ev.soc.is_none()) {
                let basis = if reading.live { SocBasis::Live } else { SocBasis::Read };
                self.ev.soc = Some(EvSoc { soc: reading.soc, at: now, basis });
                return;
            }
        }
        if let Some(soc) = self.ev.soc.as_mut().filter(|_| charging) {
            let hours = (now - soc.at).num_seconds().max(0) as f64 / 3600.0;
            soc.soc = (soc.soc + ev_rate_pct_per_hour(config) * hours).min(100.0);
            soc.basis = SocBasis::Estimated;
            soc.at = now;
        }
    }
}

fn load_options(config: &AutomationConfig) -> Vec<(LoadKind, Vec<f64>)> {
    let mut options = Vec::new();
    if config.water_reminder_enabled {
        options.push((LoadKind::Water, vec![config.water_motor_watts]));
    }
    if config.geyser_enabled {
        options.push((LoadKind::Geyser, geyser_levels(config)));
    }
    if config.ev_enabled {
        options.push((LoadKind::Ev, vec![config.ev_charger_watts]));
    }
    options
}

fn sunset_target(input: &LoadsInput) -> f64 {
    input
        .tonight_needs_soc
        .map_or(100.0, |needs| (input.config.min_soc_percent + needs + SUNSET_MARGIN_SOC).min(100.0))
}

fn fallback_ev_charge(soc: f64, config: &AutomationConfig, today_spare_wh: Option<f64>, tomorrow_spare_kwh: Option<f64>) -> EvCharge {
    let need_kwh = ev_need_wh(soc, config) / 1000.0;
    let low = soc < config.ev_daily_use_pct * 2.0;
    let tomorrow_short = tomorrow_spare_kwh.is_some_and(|spare| spare < need_kwh) && today_spare_wh.is_some_and(|spare| spare > 0.0);
    if low || tomorrow_short { EvCharge::Yes } else { EvCharge::IfSpare }
}

fn level_text(watts: f64, config: &AutomationConfig) -> String {
    let both = config.geyser_low_watts + config.geyser_high_watts;
    if (watts - both).abs() < 1.0 {
        format!("{watts:.0} W (both elements)")
    } else {
        format!("{watts:.0} W")
    }
}

struct Verdict {
    phase: LoadPhase,
    note: String,
    watts: Option<f64>,
    slot: Option<Slot>,
}

fn row_from(verdict: &Verdict, running: Option<&Running>, memory: Option<&RowMemory>) -> LoadRow {
    LoadRow {
        phase: verdict.phase,
        note: Some(verdict.note.clone()),
        watts: verdict.watts,
        from: verdict.slot.as_ref().map(|slot| slot.start),
        until: verdict.slot.as_ref().map(|slot| slot.end),
        running_w: running.map(|r| r.watts),
        running_since: running.map(|r| r.since),
        last_reminded_at: memory.and_then(|m| m.last_reminded_at),
    }
}

/// What a planned or running load should do now: run (Ready), keep going,
/// wait for its slot, or come off (Stop) because the sun can't carry it.
fn judge(slot: Option<&Slot>, running: Option<&Running>, now: NaiveDateTime) -> (LoadPhase, Option<f64>) {
    let current = slot.filter(|slot| slot.start <= now + Duration::minutes(1) && now < slot.end);
    match (current, running) {
        (Some(slot), Some(running)) if slot.watts < running.watts - 1.0 => (LoadPhase::Stop, Some(slot.watts)),
        (Some(_), Some(running)) => (LoadPhase::Running, Some(running.watts)),
        (Some(slot), None) => (LoadPhase::Ready, Some(slot.watts)),
        (None, Some(_)) => (LoadPhase::Stop, None),
        (None, None) if slot.is_some() => (LoadPhase::Waiting, slot.map(|s| s.watts)),
        (None, None) => (LoadPhase::Skipped, None),
    }
}

struct Notices<'a> {
    now: NaiveDateTime,
    repeat: Duration,
    rows: &'a mut BTreeMap<LoadKind, RowMemory>,
    out: Vec<(&'static str, String)>,
}

impl Notices<'_> {
    /// Ready repeats every `repeat`; Stop only once it has held a while, so a
    /// passing cloud doesn't send "switch off", then "switch on" again.
    fn track(&mut self, kind: LoadKind, title: &'static str, phase: LoadPhase, message: &str, quiet: bool, repeat: Duration) {
        let now = self.now;
        let repeat = repeat.max(self.repeat);
        let row = self.rows.entry(kind).or_default();
        let entered = row.phase != phase;
        row.phase = phase;
        match phase {
            LoadPhase::Ready => {
                row.stop_since = None;
                if entered {
                    row.ready_at = Some(now);
                }
                let after_stop = row.last_stop_at.is_none_or(|at| now - at >= Duration::minutes(READY_AFTER_STOP_MINUTES));
                let due = row.last_reminded_at.is_none_or(|at| entered || now - at >= repeat);
                if due && after_stop {
                    row.last_reminded_at = Some(now);
                    if !quiet {
                        self.out.push((title, message.to_string()));
                    }
                }
            }
            LoadPhase::Stop => {
                let since = *row.stop_since.get_or_insert(now);
                let held = now - since >= Duration::minutes(STOP_NOTIFY_AFTER_MINUTES);
                let due = row.last_stop_at.is_none_or(|at| now - at >= repeat);
                if held && due {
                    row.last_stop_at = Some(now);
                    row.last_reminded_at = Some(now);
                    if !quiet {
                        self.out.push((title, message.to_string()));
                    }
                }
            }
            _ => row.stop_since = None,
        }
    }
}

fn geyser_need_wh(memory: &LoadsMemory, config: &AutomationConfig, running: bool) -> f64 {
    let low = config.geyser_low_watts.min(config.geyser_high_watts);
    if memory.geyser_heated_at.is_some() {
        return if running { low * GEYSER_TOP_UP_MINUTES as f64 / 60.0 } else { 0.0 };
    }
    (config.geyser_daily_kwh * 1000.0 - memory.geyser_delivered_wh).max(low * MIN_RUN_MINUTES as f64 / 60.0)
}

fn next_days(input: &LoadsInput, house_kwh: f64, battery_kwh: f64) -> Vec<EvDayRow> {
    let Some(forecast) = input.forecast else {
        return Vec::new();
    };
    let config = input.config;
    let today = input.now.date();
    let fixed_kwh = if config.geyser_enabled { config.geyser_daily_kwh } else { 0.0 }
        + if config.water_reminder_enabled { config.water_motor_watts * config.water_run_minutes as f64 / 60_000.0 } else { 0.0 };
    forecast
        .daily
        .iter()
        .filter(|day| day.date > today && day.date <= today + Duration::days(3))
        .map(|day| {
            let pv = day
                .radiation_kwh_m2
                .filter(|_| config.pv_array_watts > 0.0)
                .map(|kwh_m2| kwh_m2 * config.pv_array_watts / 1000.0 * input.pv_ratio);
            EvDayRow {
                date: day.date.format("%a %d %b").to_string(),
                relative: if day.date == today + Duration::days(1) { "tomorrow".into() } else { day.date.format("%A").to_string() },
                radiation_kwh_m2: day.radiation_kwh_m2.map(|v| (v * 10.0).round() / 10.0),
                sunshine_h: day.sunshine_h.map(|v| (v * 10.0).round() / 10.0),
                cloud_pct: forecast_cloud(forecast, day.date),
                rain_prob_pct: day.rain_prob_pct,
                expected_pv_kwh: pv.map(|v| (v * 10.0).round() / 10.0),
                expected_spare_kwh: pv.map(|v| ((v - house_kwh - battery_kwh - fixed_kwh).max(0.0) * 10.0).round() / 10.0),
            }
        })
        .collect()
}

fn forecast_cloud(forecast: &Forecast, date: NaiveDate) -> Option<f64> {
    let clouds: Vec<f64> = forecast
        .hourly
        .iter()
        .filter(|h| h.at.date() == date && (9..=16).contains(&h.at.hour()))
        .filter_map(|h| h.cloud_pct)
        .collect();
    (!clouds.is_empty()).then(|| (clouds.iter().sum::<f64>() / clouds.len() as f64).round())
}

pub fn step(memory: &mut LoadsMemory, input: &LoadsInput) -> LoadsStep {
    let config = input.config;
    let now = input.now;
    let today = now.date();
    memory.new_day(today);
    let mut result = LoadsStep::default();

    let elapsed_h = memory.last_at.map_or(0.0, |at| (now - at).num_seconds().clamp(0, MAX_GAP_MINUTES * 60) as f64 / 3600.0);
    memory.last_at = Some(now);
    if let Some(running) = memory.detector.running(LoadKind::Geyser) {
        memory.geyser_delivered_wh += running.watts * elapsed_h;
    }
    let ev_was = memory.ev.soc.clone();
    let expected = memory.expected(now);
    let options = load_options(config);
    let changes = match input.load_w {
        Some(load) => memory.detector.observe(now, load, &options, &expected),
        None => Vec::new(),
    };
    let mut infos: Vec<(&'static str, String)> = Vec::new();
    let asked_off = |memory: &LoadsMemory, kind: LoadKind| memory.rows.get(&kind).is_some_and(|row| row.phase == LoadPhase::Stop);
    let (geyser_asked_off, ev_asked_off) = (asked_off(memory, LoadKind::Geyser), asked_off(memory, LoadKind::Ev));
    for change in &changes {
        tracing::info!("loads: {change:?} at {}", clock_time(now));
        match change {
            Change::Stopped(LoadKind::Geyser, _) if !geyser_asked_off && memory.geyser.pending.is_none() => {
                if memory.geyser_heated_at.is_none() && input.daytime {
                    memory.geyser_heated_at = Some(now);
                    infos.push((
                        GEYSER_TITLE,
                        format!(
                            "The water is hot — the geyser's thermostat switched it off at {} ({:.1} kWh today).",
                            clock_time(now),
                            memory.geyser_delivered_wh / 1000.0
                        ),
                    ));
                }
            }
            Change::Stopped(LoadKind::Ev, _) if !ev_asked_off => {
                let estimate = ev_was.as_ref().map(|soc| soc.soc);
                if estimate.is_some_and(|soc| soc >= EV_FULL_GUESS_SOC) {
                    memory.ev.soc = Some(EvSoc { soc: 100.0, at: now, basis: SocBasis::Full });
                    infos.push((EV_TITLE, "Scooter charged — the charger switched itself off.".into()));
                }
            }
            Change::Started(LoadKind::Ev, _) => {
                if let Some(soc) = memory.ev.soc.as_mut() {
                    soc.at = now;
                }
                result.ev_read = Some(false);
            }
            _ => {}
        }
    }

    let charging = memory.detector.running(LoadKind::Ev).is_some();
    memory.update_ev_soc(input.ev_reading.as_ref(), charging, now, config);

    let water_running = memory.detector.running(LoadKind::Water).cloned();
    let geyser_running = memory.detector.running(LoadKind::Geyser).cloned();
    let ev_running = memory.detector.running(LoadKind::Ev).cloned();
    let flexible_w: f64 = [&water_running, &geyser_running, &ev_running].iter().filter_map(|r| r.as_ref().map(|r| r.watts)).sum();

    let soc_ready = input.soc.is_some();
    let battery_full = input.soc.is_some_and(|soc| soc >= FULL_SOC);
    let charge_capped = match (input.charge_a, input.max_charge_a) {
        (Some(charge), Some(max)) => charge >= max * 0.9,
        _ => false,
    };
    let forecast_pv = |at: NaiveDateTime| {
        let forecast = input.forecast?;
        (config.pv_array_watts > 0.0)
            .then(|| forecast.radiation_during(at))
            .flatten()
            .map(|radiation| radiation * config.pv_array_watts / 1000.0 * input.pv_ratio)
    };
    let pv_now_w = match (input.pv_w, forecast_pv(now)) {
        (Some(live), Some(forecast)) if battery_full || charge_capped => Some(live.max(forecast)),
        (Some(live), _) => Some(live),
        (None, forecast) => forecast,
    };
    let house_now_w = input.load_w.map(|load| (load - flexible_w).max(0.0));

    let ev_soc = memory.ev.soc.as_ref().map(|soc| soc.soc);
    let decision = memory.ev.decision.as_ref().filter(|d| d.at.date() == today).cloned();
    let geyser_done = input.geyser_done_on == Some(today);
    let geyser_need = if config.geyser_enabled && !geyser_done { geyser_need_wh(memory, config, geyser_running.is_some()) } else { 0.0 };
    let ev_skipped = input.ev_skip_on == Some(today) && ev_running.is_none();
    let ev_need = if config.ev_enabled && !ev_skipped { ev_soc.map_or(0.0, |soc| ev_need_wh(soc, config)) } else { 0.0 };
    let water_need = if input.water_pending || water_running.is_some() {
        config.water_motor_watts * config.water_run_minutes as f64 / 60.0
    } else {
        0.0
    };

    let sunset = input.sunset;
    let geyser_until = at_hour(today, config.geyser_end_hour).min(sunset);
    let needs_for = |charge: EvCharge| -> Vec<Need> {
        let charge = if charge == EvCharge::No && ev_running.is_some() { EvCharge::IfSpare } else { charge };
        let ev = |priority: EvCharge| Need {
            kind: LoadKind::Ev,
            levels: vec![config.ev_charger_watts],
            need_wh: if charge == priority { ev_need } else { 0.0 },
            from: now,
            until: sunset,
            running_w: ev_running.as_ref().map(|r| r.watts),
            partial: true,
        };
        vec![
            Need {
                kind: LoadKind::Water,
                levels: vec![config.water_motor_watts],
                need_wh: water_need,
                from: at_hour(today, config.water_start_hour),
                until: at_hour(today, config.water_end_hour).min(sunset),
                running_w: water_running.as_ref().map(|r| r.watts),
                partial: water_running.is_some(),
            },
            ev(EvCharge::Yes),
            Need {
                kind: LoadKind::Geyser,
                levels: geyser_levels(config),
                need_wh: geyser_need,
                from: at_hour(today, config.geyser_start_hour),
                until: geyser_until,
                running_w: geyser_running.as_ref().map(|r| r.watts),
                partial: true,
            },
            ev(EvCharge::IfSpare),
        ]
    };

    let typical = input.typical_load;
    let pv_at = |at: NaiveDateTime| forecast_pv(at);
    let target = sunset_target(input);
    let sun = Sun {
        now,
        sunset,
        soc: input.soc.unwrap_or(0.0),
        rated_ah: input.rated_ah,
        volts: input.volts,
        max_charge_a: input.max_charge_a,
        floor: config.min_soc_percent,
        sunset_target: target,
        pv_now_w,
        house_now_w,
        pv_at: &pv_at,
        house_at: typical,
    };
    let daytime = input.daytime && soc_ready && now < sunset;
    let probe = daytime.then(|| plan(&sun, &needs_for(EvCharge::IfSpare))).flatten();

    let upcoming = {
        let house_kwh = (7..17).filter_map(typical).sum::<f64>() / 1000.0;
        let battery_kwh = input.tonight_needs_soc.unwrap_or(50.0) / 100.0 * input.rated_ah * input.volts / 1000.0;
        next_days(input, house_kwh, battery_kwh)
    };
    let ev_charge = decision.as_ref().map(|d| d.charge).unwrap_or_else(|| {
        ev_soc.map_or(EvCharge::IfSpare, |soc| {
            fallback_ev_charge(soc, config, probe.as_ref().map(|p| p.spare_before_ev_wh), upcoming.first().and_then(|d| d.expected_spare_kwh))
        })
    });
    let planned = daytime.then(|| plan(&sun, &needs_for(ev_charge))).flatten();
    let slot_for = |kind: LoadKind| planned.as_ref().and_then(|p| p.slots.iter().find(|s| s.kind == kind).cloned());

    let mut notices = Notices {
        now,
        repeat: Duration::minutes(i64::from(config.water_repeat_minutes)),
        rows: &mut memory.rows,
        out: Vec::new(),
    };

    let auto = config.geyser_auto_control && input.remote_ready && memory.geyser.failed_on != Some(today);
    let night_repeat = Duration::hours(12);
    let day_repeat = Duration::zero();
    let geyser_verdict = if !config.geyser_enabled {
        Verdict { phase: LoadPhase::Off, note: "Geyser planning is off.".into(), watts: None, slot: None }
    } else if !soc_ready && input.daytime {
        Verdict { phase: LoadPhase::Waiting, note: "Waiting for the battery level.".into(), watts: None, slot: None }
    } else if !input.daytime || now >= sunset {
        match &geyser_running {
            Some(_) => Verdict { phase: LoadPhase::Stop, note: "Switch the geyser off for the night.".into(), watts: None, slot: None },
            None => Verdict {
                phase: if memory.geyser_heated_at.is_some() || geyser_done { LoadPhase::Done } else { LoadPhase::Waiting },
                note: "Off for the night. Heating is planned in the sun from tomorrow morning.".into(),
                watts: None,
                slot: None,
            },
        }
    } else if (geyser_done || memory.geyser_heated_at.is_some()) && geyser_running.is_none() {
        let note = match memory.geyser_heated_at {
            Some(at) => format!("Hot — the thermostat cut out at {} after {:.1} kWh.", clock_time(at), memory.geyser_delivered_wh / 1000.0),
            None => "Marked done for today.".into(),
        };
        Verdict { phase: LoadPhase::Done, note, watts: None, slot: None }
    } else {
        let slot = slot_for(LoadKind::Geyser);
        let (phase, watts) = judge(slot.as_ref(), geyser_running.as_ref(), now);
        let note = match (phase, &slot) {
            (LoadPhase::Ready, Some(slot)) => format!(
                "Turn the geyser on at {}. The sun can carry it until about {}.",
                level_text(slot.watts, config),
                clock_time(slot.end)
            ),
            (LoadPhase::Running, Some(slot)) => format!("Heating at {}. Fine until about {}.", level_text(slot.watts, config), clock_time(slot.end)),
            (LoadPhase::Stop, Some(slot)) if watts.is_some() => format!(
                "Turn the geyser down to {} — the sun can't carry {} right now.",
                level_text(slot.watts, config),
                level_text(geyser_running.as_ref().map_or(0.0, |r| r.watts), config)
            ),
            (LoadPhase::Stop, _) => "Switch the geyser off — the sun can't carry it now without using battery tonight needs.".to_string(),
            (LoadPhase::Waiting, Some(slot)) => format!(
                "Best sun for it: {} to {} at {}.",
                clock_time(slot.start),
                clock_time(slot.end),
                level_text(slot.watts, config)
            ),
            _ if now >= geyser_until => "Today's heating window is over.".to_string(),
            _ => "Not enough sun left today to heat it without using battery tonight needs.".to_string(),
        };
        Verdict { phase, note, watts, slot }
    };
    if geyser_verdict.phase == LoadPhase::Skipped && soc_ready && !memory.geyser_weak_notified && now < geyser_until && input.forecast.is_some() {
        memory.geyser_weak_notified = true;
        infos.push((GEYSER_TITLE, format!("{} Use gas or heat it later from the grid at 800 W if you need it.", geyser_verdict.note)));
    }
    let repeat = if daytime { day_repeat } else { night_repeat };
    notices.track(LoadKind::Geyser, GEYSER_TITLE, geyser_verdict.phase, &geyser_verdict.note, auto, repeat);

    let ev_verdict = if !config.ev_enabled {
        Verdict { phase: LoadPhase::Off, note: "Scooter planning is off.".into(), watts: None, slot: None }
    } else if ev_soc.is_none() {
        let note = if ev_running.is_some() {
            "Charging, but the scooter's % isn't known yet — tap Read now."
        } else {
            "The scooter's % isn't known yet — tap Read now when it's near the Mac."
        };
        Verdict { phase: LoadPhase::Waiting, note: note.into(), watts: None, slot: None }
    } else if !input.daytime || now >= sunset {
        match &ev_running {
            Some(_) => Verdict { phase: LoadPhase::Stop, note: "Unplug the scooter — it charges from solar only, and the sun is down.".into(), watts: None, slot: None },
            None => Verdict { phase: LoadPhase::Waiting, note: "Charging is planned in the sun from tomorrow morning.".into(), watts: None, slot: None },
        }
    } else if input.ev_skip_on == Some(today) && ev_running.is_none() {
        Verdict { phase: LoadPhase::Skipped, note: "Skipped for today.".into(), watts: None, slot: None }
    } else if ev_soc.is_some_and(|soc| soc >= FULL_SOC) && ev_running.is_none() {
        Verdict { phase: LoadPhase::Done, note: "Full.".into(), watts: None, slot: None }
    } else if ev_charge == EvCharge::No && ev_running.is_none() {
        let reason = decision.as_ref().map(|d| d.reason.clone()).unwrap_or_default();
        Verdict { phase: LoadPhase::Skipped, note: format!("No need to charge today. {reason}").trim().to_string(), watts: None, slot: None }
    } else {
        let slot = slot_for(LoadKind::Ev);
        let (phase, watts) = judge(slot.as_ref(), ev_running.as_ref(), now);
        let hours = ev_need_wh(ev_soc.unwrap_or(100.0), config) / config.ev_charger_watts;
        let note = match (phase, &slot) {
            (LoadPhase::Ready, Some(slot)) => format!(
                "Plug in the scooter ({:.0}%, about {} to full). The sun can carry the charger until about {}.",
                ev_soc.unwrap_or_default(),
                duration_text(hours),
                clock_time(slot.end)
            ),
            (LoadPhase::Running, Some(slot)) => format!("Charging on solar until about {}.", clock_time(slot.end)),
            (LoadPhase::Stop, _) => "Unplug the scooter — the sun can't carry the charger right now.".to_string(),
            (LoadPhase::Waiting, Some(slot)) => format!("Best sun for charging: {} to {}.", clock_time(slot.start), clock_time(slot.end)),
            _ => "No spare sun for the scooter today.".to_string(),
        };
        Verdict { phase, note, watts, slot }
    };
    notices.track(LoadKind::Ev, EV_TITLE, ev_verdict.phase, &ev_verdict.note, false, repeat);
    let mut out = std::mem::take(&mut notices.out);
    out.extend(infos);
    result.notify = out;

    if auto {
        result.geyser_command = geyser_command(memory, &geyser_verdict, geyser_running.as_ref(), now, &mut result.notify);
    } else {
        memory.geyser.pending = None;
    }

    if config.ev_enabled && !input.ev_reader_busy {
        let since = |at: Option<NaiveDateTime>, minutes: i64| at.is_none_or(|at| now - at >= Duration::minutes(minutes));
        let live_due = charging && config.ev_power_cycle_reads && since(memory.ev.last_live_request, EV_LIVE_READ_MINUTES);
        let passive_due = since(memory.ev.last_read_request, if input.daytime { EV_READ_MINUTES } else { EV_NIGHT_READ_MINUTES });
        if live_due {
            result.ev_read = Some(true);
        } else if result.ev_read.is_some() || passive_due {
            result.ev_read = Some(false);
        }
        match result.ev_read {
            Some(true) => {
                memory.ev.last_live_request = Some(now);
                memory.ev.last_read_request = Some(now);
            }
            Some(false) => memory.ev.last_read_request = Some(now),
            None => {}
        }
    } else {
        result.ev_read = None;
    }

    let ask = config.ev_enabled
        && !ev_skipped
        && daytime
        && memory.ev.ask_after.is_none_or(|at| now >= at)
        && ev_soc.is_some_and(|soc| soc < FULL_SOC)
        && memory.ev.decision.as_ref().is_none_or(|d| {
            d.at.date() != today || now >= d.recheck_at || ev_soc.is_some_and(|soc| (soc - d.soc).abs() >= EV_ASK_SOC_CHANGE)
        });
    if ask {
        if let (Some(soc), Some(probe)) = (memory.ev.soc.clone(), probe.as_ref()) {
            result.ev_input = Some(EvInput {
                now: now.format("%a %d %b %Y, %H:%M").to_string(),
                sunset: clock_time(sunset),
                hours_of_sun_left: (((sunset - now).num_minutes().max(0) as f64) / 6.0).round() / 10.0,
                soc: soc.soc.round(),
                soc_basis: format!("{:?}", soc.basis).to_lowercase(),
                battery_kwh: config.ev_battery_kwh,
                charger_w: config.ev_charger_watts,
                hours_to_full: (ev_need_wh(soc.soc, config) / config.ev_charger_watts * 10.0).round() / 10.0,
                daily_use_pct: config.ev_daily_use_pct,
                days_of_riding_left: ((soc.soc / config.ev_daily_use_pct) * 10.0).round() / 10.0,
                charging_now: charging,
                need_kwh: (ev_need_wh(soc.soc, config) / 100.0).round() / 10.0,
                spare_kwh_today: (probe.spare_before_ev_wh / 100.0).round() / 10.0,
                today_hours: probe
                    .hours
                    .iter()
                    .map(|(at, surplus)| EvHourRow { time: clock_time(*at), spare_w: surplus.max(0.0).round() })
                    .collect(),
                other_loads_today: other_loads_text(&geyser_verdict, input.water_pending),
                next_days: upcoming,
            });
        }
    }

    let active = [&geyser_verdict.phase, &ev_verdict.phase]
        .iter()
        .any(|phase| matches!(phase, LoadPhase::Ready | LoadPhase::Running | LoadPhase::Stop))
        || water_running.is_some()
        || memory.geyser.pending.is_some();
    result.next_at = Some(now + Duration::minutes(if active { ACTIVE_TICK_MINUTES } else { IDLE_TICK_MINUTES }));

    let soc = memory.ev.soc.as_ref();
    result.status = LoadsStatus {
        geyser: GeyserStatus {
            row: row_from(&geyser_verdict, geyser_running.as_ref(), memory.rows.get(&LoadKind::Geyser)),
            delivered_kwh: (memory.geyser_delivered_wh / 100.0).round() / 10.0,
            need_kwh: config.geyser_daily_kwh,
            heated_at: memory.geyser_heated_at,
            auto,
        },
        ev: EvStatus {
            row: row_from(&ev_verdict, ev_running.as_ref(), memory.rows.get(&LoadKind::Ev)),
            soc: soc.map(|s| s.soc),
            soc_at: soc.map(|s| s.at),
            soc_basis: soc.map(|s| s.basis),
            hours_to_full: soc.map(|s| ev_need_wh(s.soc, config) / config.ev_charger_watts),
            decision: decision.as_ref().map(|d| d.charge).or(ev_soc.map(|_| ev_charge)),
            decision_reason: decision.as_ref().map(|d| d.reason.clone()),
            decision_model: decision.as_ref().and_then(|d| d.model.clone()),
            reading: input.ev_reader_busy,
            read_error: None,
        },
        water_running_since: water_running.map(|r| r.since),
        spare_kwh: planned.as_ref().map(|p| (p.spare_wh / 100.0).round() / 10.0),
        sunset_target_soc: daytime.then_some(target),
        projected_sunset_soc: planned.as_ref().map(|p| p.end_soc.round()),
        pv_ratio: Some((input.pv_ratio * 100.0).round() / 100.0),
        plan: planned.map(|p| p.slots).unwrap_or_default(),
    };
    result
}

fn other_loads_text(geyser: &Verdict, water_pending: bool) -> String {
    let water = if water_pending { "The water tank still needs filling today." } else { "The water tank is done or not needed." };
    format!("{water} Geyser: {}", geyser.note)
}

/// With the ESP32 in charge, the plan becomes button presses. Every press is
/// checked against the load a few minutes later; two misses and control
/// goes back to reminders for the rest of the day.
fn geyser_command(
    memory: &mut LoadsMemory,
    verdict: &Verdict,
    running: Option<&Running>,
    now: NaiveDateTime,
    notify: &mut Vec<(&'static str, String)>,
) -> Option<GeyserCommand> {
    let control = &mut memory.geyser;
    if let Some((intent, sent_at, attempts)) = control.pending {
        let done = match intent {
            GeyserIntent::On(watts) => running.is_some_and(|r| (r.watts - watts).abs() < 1.0),
            GeyserIntent::Off => running.is_none(),
        };
        if done {
            control.pending = None;
        } else if now - sent_at >= Duration::minutes(GEYSER_COMMAND_WAIT_MINUTES) {
            if attempts < 2 && matches!(intent, GeyserIntent::On(_)) {
                return Some(GeyserCommand { intent, press_power: running.is_none() });
            }
            control.pending = None;
            control.failed_on = Some(now.date());
            let message = match intent {
                GeyserIntent::On(_) => "The geyser didn't start after the ESP32 pressed its buttons — it may already be hot. Back to reminders for today.",
                GeyserIntent::Off => "The geyser didn't switch off after the ESP32 pressed On/Off — switch it off by hand. Back to reminders for today.",
            };
            notify.push((GEYSER_TITLE, message.into()));
            return None;
        } else {
            return None;
        }
    }
    if control.last_sent_at.is_some_and(|at| now - at < Duration::minutes(GEYSER_COMMAND_GAP_MINUTES)) {
        return None;
    }
    let command = match (verdict.phase, running) {
        (LoadPhase::Ready, None) => GeyserCommand { intent: GeyserIntent::On(verdict.watts?), press_power: false },
        (LoadPhase::Stop, Some(_)) => match verdict.watts {
            Some(level) => GeyserCommand { intent: GeyserIntent::On(level), press_power: false },
            None => GeyserCommand { intent: GeyserIntent::Off, press_power: true },
        },
        _ => return None,
    };
    let message = match command.intent {
        GeyserIntent::On(watts) => format!("Turning the geyser to {watts:.0} W. {}", verdict.note),
        GeyserIntent::Off => format!("Switching the geyser off. {}", verdict.note),
    };
    notify.push((GEYSER_TITLE, message));
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::weather::HourWeather;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn config() -> AutomationConfig {
        AutomationConfig { pv_array_watts: 4000.0, ..AutomationConfig::default() }
    }

    fn sunny(peak: f64) -> Forecast {
        let hourly = (0..24)
            .map(|hour| {
                let radiation = if (8..=17).contains(&hour) { peak * (1.0 - ((hour as f64 - 12.5) / 5.0).powi(2)).max(0.0) } else { 0.0 };
                HourWeather { at: at(hour, 0), temp_c: None, cloud_pct: Some(10.0), radiation_w_m2: Some(radiation) }
            })
            .collect();
        Forecast { hourly, daily: Vec::new() }
    }

    fn input<'a>(now: NaiveDateTime, soc: f64, pv: f64, load: f64, forecast: Option<&'a Forecast>, config: &'a AutomationConfig) -> LoadsInput<'a> {
        LoadsInput {
            now,
            daytime: true,
            sunset: at(17, 15),
            soc: Some(soc),
            rated_ah: 100.0,
            volts: 26.0,
            max_charge_a: Some(60.0),
            charge_a: Some(10.0),
            pv_w: Some(pv),
            load_w: Some(load),
            forecast,
            pv_ratio: 0.75,
            typical_load: &|_| Some(300.0),
            tonight_needs_soc: Some(45.0),
            config,
            water_pending: false,
            geyser_done_on: None,
            ev_skip_on: None,
            ev_reading: None,
            ev_reader_busy: false,
            remote_ready: false,
        }
    }

    #[test]
    fn geyser_levels_are_both_high_and_low() {
        assert_eq!(geyser_levels(&config()), vec![2000.0, 1200.0, 800.0]);
    }

    #[test]
    fn a_sunny_midday_with_a_full_battery_says_turn_the_geyser_on() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        assert_eq!(result.status.geyser.row.phase, LoadPhase::Ready);
        assert!(result.notify.iter().any(|(title, body)| *title == GEYSER_TITLE && body.contains("Turn the geyser on")));
        assert!(result.status.projected_sunset_soc.unwrap() >= result.status.sunset_target_soc.unwrap());
    }

    #[test]
    fn a_dark_day_keeps_the_geyser_off_and_says_so_once() {
        let config = config();
        let sky = sunny(120.0);
        let mut memory = LoadsMemory::default();
        let first = step(&mut memory, &input(at(10, 30), 55.0, 150.0, 300.0, Some(&sky), &config));
        assert_eq!(first.status.geyser.row.phase, LoadPhase::Skipped);
        assert!(first.notify.iter().any(|(_, body)| body.contains("Not enough sun")));
        let again = step(&mut memory, &input(at(10, 35), 55.0, 150.0, 300.0, Some(&sky), &config));
        assert!(again.notify.is_empty());
    }

    #[test]
    fn the_battery_still_reaches_tonights_need_with_the_loads_planned() {
        let config = config();
        let sun = sunny(600.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(10, 15), 50.0, 1200.0, 300.0, Some(&sun), &config));
        let target = result.status.sunset_target_soc.unwrap();
        assert_eq!(target, 70.0);
        assert!(result.status.projected_sunset_soc.unwrap() >= target - 0.5);
    }

    #[test]
    fn detector_tells_the_loads_apart_by_their_step() {
        let config = config();
        let options = load_options(&config);
        let mut detector = Detector::default();
        assert!(detector.observe(at(11, 0), 300.0, &options, &[]).is_empty());
        assert_eq!(detector.observe(at(11, 2), 1510.0, &options, &[]), vec![Change::Started(LoadKind::Geyser, 1200.0)]);
        assert_eq!(detector.observe(at(11, 4), 2090.0, &options, &[]), vec![Change::Started(LoadKind::Ev, 600.0)]);
        assert_eq!(detector.observe(at(11, 6), 2890.0, &options, &[]), vec![Change::Level(LoadKind::Geyser, 1200.0, 2000.0)]);
        assert_eq!(detector.observe(at(11, 8), 900.0, &options, &[]), vec![Change::Stopped(LoadKind::Geyser, 2000.0)]);
        assert_eq!(detector.observe(at(11, 10), 250.0, &options, &[]), vec![Change::Stopped(LoadKind::Ev, 600.0)]);
    }

    #[test]
    fn an_expected_load_wins_a_close_match() {
        let config = config();
        let options = load_options(&config);
        let mut detector = Detector::default();
        detector.observe(at(11, 0), 300.0, &options, &[]);
        assert_eq!(detector.observe(at(11, 2), 1400.0, &options, &[LoadKind::Water]), vec![Change::Started(LoadKind::Water, 1000.0)]);
    }

    #[test]
    fn a_load_that_vanished_between_reads_is_dropped() {
        let config = config();
        let options = load_options(&config);
        let mut detector = Detector::default();
        detector.observe(at(11, 0), 300.0, &options, &[]);
        detector.observe(at(11, 2), 2300.0, &options, &[]);
        let changes = detector.observe(at(11, 40), 350.0, &options, &[]);
        assert_eq!(changes, vec![Change::Lost(LoadKind::Geyser)]);
    }

    #[test]
    fn the_thermostat_cutting_out_marks_the_water_hot() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        let running = step(&mut memory, &input(at(12, 0), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        assert_eq!(running.status.geyser.row.phase, LoadPhase::Running);
        let cut = step(&mut memory, &input(at(12, 2), 100.0, 300.0, 300.0, Some(&sun), &config));
        assert_eq!(cut.status.geyser.row.phase, LoadPhase::Done);
        assert!(cut.notify.iter().any(|(_, body)| body.contains("The water is hot")));
        assert!(cut.status.geyser.delivered_kwh >= 0.4);
    }

    #[test]
    fn the_geyser_is_switched_off_at_night() {
        let config = config();
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(18, 0), 80.0, 0.0, 300.0, None, &config));
        let mut night = input(at(18, 2), 80.0, 0.0, 1500.0, None, &config);
        night.daytime = false;
        let result = step(&mut memory, &night);
        assert_eq!(result.status.geyser.row.phase, LoadPhase::Stop);
    }

    #[test]
    fn scooter_charge_is_estimated_while_the_charger_runs() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut first = input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config);
        first.ev_reading = Some(EvReading { soc: 50.0, at: at(10, 55), live: false });
        step(&mut memory, &first);
        step(&mut memory, &input(at(11, 2), 100.0, 900.0, 900.0, Some(&sun), &config));
        let later = step(&mut memory, &input(at(12, 2), 100.0, 900.0, 900.0, Some(&sun), &config));
        let soc = later.status.ev.soc.unwrap();
        assert!((soc - 75.0).abs() < 1.0, "600 W × 0.9 on 2.16 kWh is 25%/h, got {soc}");
        assert_eq!(later.status.ev.soc_basis, Some(SocBasis::Estimated));
    }

    #[test]
    fn a_repeated_frozen_scooter_reading_is_not_news() {
        let config = config();
        let mut memory = LoadsMemory::default();
        memory.update_ev_soc(Some(&EvReading { soc: 50.0, at: at(9, 0), live: false }), false, at(9, 0), &config);
        memory.ev.soc.as_mut().unwrap().soc = 90.0;
        memory.update_ev_soc(Some(&EvReading { soc: 50.0, at: at(13, 0), live: false }), false, at(13, 0), &config);
        assert_eq!(memory.ev.soc.as_ref().unwrap().soc, 90.0, "the scooter wasn't switched on since, so 50% is stale");
        memory.update_ev_soc(Some(&EvReading { soc: 70.0, at: at(18, 0), live: false }), false, at(18, 0), &config);
        assert_eq!(memory.ev.soc.as_ref().unwrap().soc, 70.0);
    }

    #[test]
    fn pv_ratio_uses_unthrottled_charging_samples() {
        let sun = sunny(800.0);
        let samples: Vec<Sample> = (0..20)
            .map(|i| Sample {
                at: at(12, i * 2),
                soc: Some(70.0),
                load_w: Some(300.0),
                pv_w: Some(800.0 * 4.0 * 0.62),
                battery_a: Some(20.0),
                battery_v: Some(26.0),
                grid_on: Some(true),
                grid_basis: String::new(),
                mode: Some("SBG".into()),
                source: String::new(),
            })
            .collect();
        let ratio = pv_ratio(&samples, &sun, 4000.0, Some(60.0), at(0, 0)).unwrap();
        let radiation = sun.radiation_during(at(12, 0)).unwrap();
        assert!((ratio - 800.0 * 4.0 * 0.62 / (radiation * 4.0)).abs() < 0.01);
        assert_eq!(pv_ratio(&samples[..5], &sun, 4000.0, Some(60.0), at(0, 0)), None);
    }

    #[test]
    fn auto_control_presses_the_level_then_checks_it_started() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut ready = input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config);
        ready.remote_ready = true;
        let first = step(&mut memory, &ready);
        let command = first.geyser_command.unwrap();
        assert!(matches!(command.intent, GeyserIntent::On(_)));
        assert!(!command.press_power);
        memory.geyser_sent(command, at(11, 0), true);
        let mut waiting = input(at(11, 2), 100.0, 400.0, 300.0, Some(&sun), &config);
        waiting.remote_ready = true;
        assert!(step(&mut memory, &waiting).geyser_command.is_none());
        let mut retry = input(at(11, 4), 100.0, 400.0, 300.0, Some(&sun), &config);
        retry.remote_ready = true;
        let again = step(&mut memory, &retry).geyser_command.unwrap();
        assert!(again.press_power, "a level press that didn't start it is followed by On/Off");
    }
}
