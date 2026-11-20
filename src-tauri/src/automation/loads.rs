use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::automation::agent::{EvDayRow, LoadsAiInput};
use crate::automation::config::AutomationConfig;
use crate::automation::history::Sample;
use crate::automation::solar::{self, PvPoint};
use crate::automation::telemetry::{soc_per_ah, FULL_SOC};
use crate::automation::weather::{Forecast, PERFORMANCE_RATIO};
use crate::geyser::Button;

mod ai;
pub use ai::{AiMode, AiStatus, Checker, LoadsQuestion, Verified};

pub const GEYSER_TITLE: &str = "Geyser";
pub const EV_TITLE: &str = "Scooter";
pub const HOTPLATE_TITLE: &str = "Hotplate";
pub const LOADS_TITLE: &str = "Solar loads";

const STEP_MINUTES: i64 = 15;
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
const STICKY_SHARE: f64 = 0.5;
const IDLE_CHECK_SHARE: f64 = 0.6;
const EXPECT_MINUTES: i64 = 20;
const STOP_NOTIFY_AFTER_MINUTES: i64 = 10;
const READY_AFTER_STOP_MINUTES: i64 = 20;
/// A charger that switches itself off above this estimate has filled the pack.
const EV_FULL_GUESS_SOC: f64 = 80.0;
const EV_READ_MINUTES: i64 = 30;
const EV_NIGHT_READ_MINUTES: i64 = 120;
const EV_LIVE_READ_MINUTES: i64 = 60;
/// Two live reads this far apart or closer can tell charging from not.
const EV_COMPARE_HOURS: i64 = 6;
/// The scooter reads in 0.5% steps; a rise of this much is the charger.
const EV_RISE_PCT: f64 = 1.0;
const EV_CHECK_ATTEMPTS: u32 = 3;
const EV_CHECK_RETRY_MINUTES: i64 = 30;
/// Today's sun counts as able to charge the scooter when it gives this share.
const EV_SUNNY_SHARE: f64 = 0.9;
/// A motor run this long that stops makes the app ask if the tank is full.
const TANK_ASK_AFTER_MINUTES: i64 = 5;
/// Near full and barely charging, the inverter holds the panels back to the
/// house load, so live solar says nothing about what the panels could give.
const NEARLY_FULL_SOC: f64 = 95.0;
const EV_NEARLY_FULL_PCT: f64 = 95.0;
/// A morning this cold means cold water: the geyser heats today whatever the
/// sun, with the grid topping up.
const GEYSER_REQUIRED_BELOW_C: f64 = 20.0;
const OFF_SEASON_NOTE: &str = "Outside winter the app leaves the geyser alone; switch it on by hand if you want it.";
const NEARLY_FULL_IDLE_A: f64 = 5.0;
const EV_FULL_MESSAGE: &str = "Scooter full — switch the charger off at the wall first, then unplug the battery.";
const ACTIVE_TICK_MINUTES: i64 = 2;
const IDLE_TICK_MINUTES: i64 = 5;
const RATIO_MIN_SAMPLES: usize = 12;
const RATIO_MIN_FLOORS: usize = 6;
/// Most held-back readings only show the house's load; the highest few show
/// how much the sun gave at least.
const RATIO_FLOOR_PERCENTILE: f64 = 0.9;
/// Charging at this share of the highest current seen is the inverter's limit,
/// as long as that current is at least this many amps.
const CHARGE_CAP_SHARE: f64 = 0.9;
const CHARGE_CAP_MIN_A: f64 = 20.0;
const RATIO_RANGE: (f64, f64) = (0.35, 2.0);
const RATIO_MIN_RADIATION: f64 = 300.0;
const RATIO_PERCENTILE: f64 = 0.75;
const GEYSER_COMMAND_WAIT_MINUTES: i64 = 3;
const GEYSER_COMMAND_GAP_MINUTES: i64 = 5;
const GEYSER_WATCH_MINUTES: i64 = 10;
/// A geyser that goes off by itself is taken as hot only after this share of
/// the day's need, and at least this much, has gone in.
const GEYSER_HOT_SHARE: f64 = 0.25;
const GEYSER_HOT_MIN_WH: f64 = 500.0;
/// Battery flow (W) still counted as "not draining" while the geyser runs on sun.
const GEYSER_SUN_DRAIN_W: f64 = 150.0;
const GEYSER_UNKNOWN_LOAD_W: f64 = 400.0;
/// With the battery full the geyser goes for the biggest level within this
/// much of the spare sun; a drain steps it down again.
const GEYSER_LEND_W: f64 = 400.0;
/// A level up waits this long after the last press, and longer after a step down.
const GEYSER_STEP_UP_MINUTES: i64 = 10;
const GEYSER_UP_AFTER_DOWN_MINUTES: i64 = 30;
/// What the app believed is on is picked up again after a restart if it was
/// saved this recently.
const SAVED_LOADS_FRESH_MINUTES: i64 = 20;
/// Load readings to see after a start before the ESP32 presses anything: the
/// first one may be from before the restart.
const READINGS_BEFORE_PRESSING: u32 = 2;
const READINGS_BEFORE_STEP_UP: u32 = 4;
const GEYSER_NOT_HOTPLATE_MINUTES: i64 = 90;
const GEYSER_ASK_GAP_MINUTES: i64 = 30;
/// With the ESP32 in charge, a step in the load is only read as the geyser
/// coming on or changing level this long after the app pressed or the
/// family's remote was heard; otherwise it is another load.
const GEYSER_FREE_MINUTES: i64 = 10;
/// The share of the inverter's maximum the house is kept under.
const INVERTER_BUDGET: f64 = 0.9;
/// Load the app can't place above this is taken as the hotplate, which can be
/// turned up to its top level at any moment. A step read as the water motor
/// may be the hotplate too: the load alone can't tell them apart.
const HOTPLATE_SEEN_W: f64 = 600.0;
const HOTPLATE_GEYSER_OFF_W: f64 = 3300.0;
const HOTPLATE_GEYSER_ON_W: f64 = 3000.0;
const HOTPLATE_KEEP_BELOW_W: f64 = 1800.0;
/// A drain this big with the geyser on turns it down at once, watch or not.
const GEYSER_DRAIN_ACT_W: f64 = 300.0;
/// How long the AI gets to answer a drain before the app turns the geyser down itself.
const GEYSER_DRAIN_GRACE_MINUTES: i64 = 3;
/// The inverter cloud only reports every few minutes; a press is judged on a
/// reading taken at least this long after it.
const GEYSER_SETTLE_SECONDS: i64 = 90;
const GEYSER_DECISION_WAIT_MINUTES: i64 = 3;
const GEYSER_TOP_UP_MINUTES: i64 = 30;
/// Once hot, the geyser is heated once more on spare sun in the last hours
/// of its window, so the tank goes into the night at the thermostat's top.
const GEYSER_REHEAT_HOURS_BEFORE_END: i64 = 3;
const GEYSER_REHEAT_LAST_MINUTES: i64 = 30;
const GEYSER_REHEAT_AFTER_HOT_MINUTES: i64 = 60;
const UNPLACED_MINUTES: i64 = 60;
const HEARD_SAME_PRESS_SECONDS: i64 = 5;
const HOTPLATE_MAX_W: f64 = 2200.0;
const HOTPLATE_MIN_W: f64 = 400.0;
/// The settings on the hotplate's panel, highest first.
pub(super) const HOTPLATE_SETTINGS: [f64; 9] = [2000.0, 1800.0, 1600.0, 1400.0, 1200.0, 1000.0, 800.0, 400.0, 200.0];
/// The hotplate counts as off once its share of the load stays under this.
const HOTPLATE_OFF_W: f64 = 200.0;
const HOTPLATE_OFF_MINUTES: i64 = 10;
/// After a "no", the app waits this long before asking about the hotplate again.
const HOTPLATE_ASK_GAP_MINUTES: i64 = 60;
const HOTPLATE_CONFIRM_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadKind {
    Water,
    Geyser,
    Ev,
    Hotplate,
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
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SocBasis {
    Read,
    Live,
    Estimated,
    Full,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditState {
    On,
    Off,
    Done,
}

/// The family setting a load's state by hand: always taken as fact.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LoadEdit {
    pub load: LoadKind,
    pub state: EditState,
    #[serde(default)]
    pub watts: Option<f64>,
    #[serde(default)]
    pub kwh: Option<f64>,
    #[serde(default)]
    pub soc: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvCharge {
    Yes,
    IfSpare,
}

impl EvCharge {
    pub fn parse(text: &str) -> EvCharge {
        match text.trim().to_ascii_lowercase().as_str() {
            "yes" => EvCharge::Yes,
            _ => EvCharge::IfSpare,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadsAnswer {
    Yes,
    No,
    HotplateDone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slot {
    pub kind: LoadKind,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub watts: f64,
    pub partial: bool,
    /// More than the spare sun: the grid tops it up in Solar mode.
    pub topped_up: bool,
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
    pub check: Option<GeyserCheck>,
    pub watch_until: Option<NaiveDateTime>,
    pub elements: Option<GeyserElements>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Checking,
    Confirmed,
    NotConfirmed,
}

/// The last thing the ESP32 was asked to do and whether the load showed it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GeyserCheck {
    pub action: String,
    pub sent_at: NaiveDateTime,
    pub state: CheckState,
    pub checked_at: Option<NaiveDateTime>,
    pub note: String,
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
    /// What the last two live reads say: charging or not.
    pub charge_check: Option<String>,
    /// The family's rule for today: charge or not, and why.
    pub charge_today: Option<bool>,
    pub charge_rule: Option<String>,
    /// Today's automatic live read, or that it couldn't reach the scooter.
    pub checked_at: Option<NaiveDateTime>,
    pub check_failed: bool,
}

/// One hour of today's sun for the spare-sun chart: what the panels should
/// give, what the house itself takes, and what the loads on now take.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SunHour {
    pub at: NaiveDateTime,
    pub sun_w: f64,
    pub house_w: f64,
    pub loads_w: f64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct HotplateStatus {
    #[serde(flatten)]
    pub row: LoadRow,
    pub cooking_day: bool,
    pub asking_w: Option<f64>,
    pub asking_since: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct LoadsStatus {
    pub geyser: GeyserStatus,
    pub ev: EvStatus,
    pub hotplate: HotplateStatus,
    /// The water tank's row when the AI plans it.
    pub water: LoadRow,
    /// Why the geyser heats ahead of the hotplate today, if it does.
    pub geyser_first: Option<String>,
    pub question: Option<LoadsQuestion>,
    pub ai: AiStatus,
    pub water_running_since: Option<NaiveDateTime>,
    pub spare_kwh: Option<f64>,
    pub sunset_target_soc: Option<f64>,
    pub projected_sunset_soc: Option<f64>,
    pub full_by: Option<NaiveDateTime>,
    pub pv_ratio: Option<f64>,
    pub plan: Vec<Slot>,
    pub sun_hours: Vec<SunHour>,
    /// The motor ran and stopped: is the tank filled?
    pub tank_question: Option<NaiveDateTime>,
    /// A load on now takes more than the sun and the battery can't lend it:
    /// Solar mode, so the grid tops it up.
    pub grid_now: bool,
    pub grid_kwh: Option<f64>,
    /// Why the geyser runs today whatever the sun, when it does.
    pub geyser_required: Option<String>,
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

/// The element buttons are toggles, one per heating element; the geyser stays powered on.
#[derive(Debug, Clone, PartialEq)]
pub struct GeyserCommand {
    pub intent: GeyserIntent,
    /// Pressed in this order.
    pub buttons: Vec<Button>,
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
    /// Signed battery power: positive charging, negative draining.
    pub battery_w: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub forecast: Option<&'a Forecast>,
    pub pv_ratio: f64,
    /// Today's expected panel output, already pulled towards the last
    /// readings; empty falls back to the plain forecast.
    pub pv_curve: &'a [PvPoint],
    pub sky_factor: Option<f64>,
    pub typical_load: &'a dyn Fn(u32) -> Option<f64>,
    pub tonight_needs_soc: Option<f64>,
    pub config: &'a AutomationConfig,
    pub water_pending: bool,
    pub geyser_done_on: Option<NaiveDate>,
    pub ev_skip_on: Option<NaiveDate>,
    pub hotplate_on: Option<NaiveDate>,
    pub hotplate_skip_on: Option<NaiveDate>,
    /// The day the scooter's daily live read was last done.
    pub ev_checked_on: Option<NaiveDate>,
    pub answer: Option<LoadsAnswer>,
    pub edits: &'a [LoadEdit],
    pub ai_available: bool,
    /// The last readings, oldest first, for the AI to read the load's history.
    pub recent: &'a [Sample],
    pub ev_reading: Option<EvReading>,
    pub ev_reader_busy: bool,
    pub remote_ready: bool,
    /// The family asked the ESP32 to switch the geyser on at a level, or off.
    pub geyser_request: Option<GeyserIntent>,
    /// When the ESP32 last heard the family's own remote.
    pub geyser_heard_at: Option<NaiveDateTime>,
    /// Which button that was, when the ESP32 could tell.
    pub geyser_heard_button: Option<Button>,
}

/// What a yes/no answer to a question is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskAbout {
    /// The AI's question, or whether a new load is the hotplate.
    Loads,
    /// Whether the tank is full now that the motor has stopped.
    Tank,
}

/// A question the family can answer from the notification itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub title: &'static str,
    pub body: String,
    pub about: AskAbout,
}

#[derive(Debug, Default)]
pub struct LoadsStep {
    pub status: LoadsStatus,
    pub notify: Vec<(&'static str, String)>,
    pub asks: Vec<Ask>,
    pub geyser_command: Option<GeyserCommand>,
    /// Some(power_cycle) asks for a scooter read.
    pub ev_read: Option<bool>,
    pub ai_request: Option<(LoadsAiInput, Checker)>,
    /// The hotplate is on today: the day counts as a cooking day.
    pub hotplate_cooked: bool,
    /// Today's daily scooter read is done: the runner saves the day.
    pub ev_checked: bool,
    pub next_at: Option<NaiveDateTime>,
    /// One line for the loads log: what the app saw, believed and did.
    pub log: Option<serde_json::Value>,
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
/// radiation, learned from the last days. A full battery holds the panels
/// down, so only samples where it was still charging count; one charging at
/// its limit counts too, as a floor on what the panels could give.
/// How much of the forecast the panels really give. Only readings where the
/// panels ran free count as the measure: while the battery charges at the
/// inverter's current limit, or is full, the panels are held back, so those
/// readings only say the sun gave at least that much.
pub fn pv_ratio(samples: &[Sample], forecast: &Forecast, pv_array_watts: f64, since: NaiveDateTime) -> Option<f64> {
    if pv_array_watts <= 0.0 {
        return None;
    }
    let recent: Vec<&Sample> = samples.iter().filter(|s| s.at >= since).collect();
    let cap = recent.iter().filter_map(|s| s.battery_a).fold(0.0, f64::max);
    let capped = |s: &Sample| cap >= CHARGE_CAP_MIN_A && s.battery_a.is_some_and(|amps| amps >= cap * CHARGE_CAP_SHARE);
    let ratio = |s: &Sample| {
        let pv = s.pv_w.filter(|pv| *pv >= 300.0)?;
        let radiation = forecast.radiation_during(s.at).filter(|r| *r >= RATIO_MIN_RADIATION)?;
        Some(pv / (radiation * pv_array_watts / 1000.0))
    };
    let free: Vec<f64> = recent
        .iter()
        .filter(|s| s.soc.is_some_and(|soc| soc < 95.0) && s.battery_a.is_some_and(|amps| amps >= 1.0) && !capped(s))
        .filter_map(|s| ratio(s))
        .collect();
    let floors: Vec<f64> = recent
        .iter()
        .filter(|s| capped(s) || s.soc.is_some_and(|soc| soc >= 95.0))
        .filter_map(|s| ratio(s))
        .collect();
    let pick = |mut ratios: Vec<f64>, least: usize, percentile: f64| {
        (ratios.len() >= least).then(|| {
            ratios.sort_by(f64::total_cmp);
            ratios[((ratios.len() - 1) as f64 * percentile).round() as usize]
        })
    };
    let learned = match (pick(free, RATIO_MIN_SAMPLES, RATIO_PERCENTILE), pick(floors, RATIO_MIN_FLOORS, RATIO_FLOOR_PERCENTILE)) {
        (Some(free), Some(floor)) => free.max(floor),
        (one, other) => one.or(other)?,
    };
    Some(learned.clamp(RATIO_RANGE.0, RATIO_RANGE.1))
}

pub fn default_pv_ratio() -> f64 {
    PERFORMANCE_RATIO
}

#[derive(Debug, Clone, Copy)]
struct Step {
    start: NaiveDateTime,
    hours: f64,
    surplus_w: f64,
    pv_w: f64,
    house_w: f64,
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
    day_low: f64,
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
        let (pv_w, house_w) = (pv.unwrap_or(0.0), house.unwrap_or(0.0));
        out.push(Step { start: at, hours: (end - at).num_seconds() as f64 / 3600.0, surplus_w: pv_w - house_w, pv_w, house_w });
        at = end;
    }
    out
}

struct Run {
    end_soc: f64,
    low_soc: f64,
    curtailed_wh: f64,
    /// When the battery last climbed back to full.
    full_at: Option<NaiveDateTime>,
}

/// The battery after a step with `net` watts to spare, and what it couldn't take.
fn charge(sun: &Sun, soc: f64, net: f64, hours: f64) -> (f64, f64) {
    let per_ah = soc_per_ah(sun.rated_ah);
    let amps = net / sun.volts;
    let taken = sun.max_charge_a.map_or(amps, |max| amps.min(max));
    let room_ah = (100.0 - soc).max(0.0) / per_ah.max(f64::EPSILON);
    let charged_ah = (taken * hours).min(room_ah);
    ((soc + charged_ah * per_ah).min(100.0), (net * hours - charged_ah * sun.volts).max(0.0))
}

fn lend(sun: &Sun, soc: f64, net: f64, hours: f64) -> f64 {
    (soc + net / sun.volts / INVERTER_EFFICIENCY * hours * soc_per_ah(sun.rated_ah)).max(0.0)
}

fn simulate(sun: &Sun, steps: &[Step], extra: &[f64]) -> Run {
    let mut soc = sun.soc;
    let mut low = soc;
    let mut curtailed_wh = 0.0;
    let mut full_at = (soc >= FULL_SOC).then_some(sun.now);
    for (step, extra) in steps.iter().zip(extra) {
        let before = soc;
        let net = step.surplus_w - extra;
        if net >= 0.0 {
            let (after, wasted) = charge(sun, soc, net, step.hours);
            curtailed_wh += wasted;
            soc = after;
        } else {
            soc = lend(sun, soc, net, step.hours);
        }
        if soc >= FULL_SOC && before < FULL_SOC {
            full_at = Some(step.end());
        }
        low = low.min(soc);
    }
    Run { end_soc: soc, low_soc: low, curtailed_wh, full_at }
}

struct Limits {
    target: f64,
    low: f64,
}

fn fits(sun: &Sun, steps: &[Step], extra: &[f64], limits: &Limits) -> bool {
    let run = simulate(sun, steps, extra);
    run.end_soc >= limits.target - 1e-6 && run.low_soc >= limits.low - 1e-6
}

struct Hybrid {
    end_soc: f64,
    full_at: Option<NaiveDateTime>,
    grid_wh: f64,
    /// The loads on now need the grid: Solar mode.
    grid_now: bool,
    grid_steps: Vec<bool>,
}

/// The family's rule when a load takes more than the sun: the battery lends
/// (SBG) only while what the sun leaves later still fills it; otherwise Solar
/// mode, so the grid tops the load up and the battery is left alone.
fn simulate_hybrid(sun: &Sun, steps: &[Step], extra: &[f64], low: f64) -> Hybrid {
    let refill = |from: usize, soc: f64| {
        steps[from..].iter().zip(&extra[from..]).fold(soc, |soc, (step, extra)| {
            let net = step.surplus_w - extra;
            if net > 0.0 { charge(sun, soc, net, step.hours).0 } else { soc }
        })
    };
    let mut soc = sun.soc;
    let mut grid_wh = 0.0;
    let mut grid_now = false;
    let mut grid_steps = vec![false; steps.len()];
    let mut full_at = (soc >= FULL_SOC).then_some(sun.now);
    for (index, (step, extra)) in steps.iter().zip(extra).enumerate() {
        let before = soc;
        let net = step.surplus_w - extra;
        if net >= 0.0 {
            soc = charge(sun, soc, net, step.hours).0;
        } else {
            let lent = lend(sun, soc, net, step.hours);
            let can_lend = *extra <= 0.0 || (lent >= low - 1e-6 && refill(index + 1, lent) >= FULL_SOC - 1e-6);
            if can_lend {
                soc = lent;
            } else {
                grid_wh += -net * step.hours;
                grid_now |= index == 0;
                grid_steps[index] = true;
            }
        }
        if soc >= FULL_SOC && before < FULL_SOC {
            full_at = Some(step.end());
        }
    }
    Hybrid { end_soc: soc, full_at, grid_wh, grid_now, grid_steps }
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
    /// A session of this many hours at whichever level fits, instead of a
    /// fixed amount of energy.
    pub session_h: Option<f64>,
    /// Runs today whatever the sun: the grid tops up what it can't carry.
    pub required: bool,
}

struct Candidate {
    start: usize,
    end: usize,
    watts: f64,
    need_wh: f64,
    delivered_wh: f64,
    battery_wh: f64,
}

fn better(a: &Candidate, b: &Candidate) -> bool {
    let tolerance = 1.0;
    if (a.delivered_wh - b.delivered_wh).abs() > tolerance {
        return a.delivered_wh > b.delivered_wh;
    }
    if a.start != b.start {
        return a.start < b.start;
    }
    if (a.battery_wh - b.battery_wh).abs() > tolerance {
        return a.battery_wh < b.battery_wh;
    }
    a.watts > b.watts
}

/// The best contiguous run for one load: as much of its need as fits, as early
/// as it can start, the battery still as full by sunset as the sun alone
/// would leave it.
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
            let need_wh = need.session_h.map_or(need.need_wh, |hours| watts * hours);
            let mut energy = 0.0;
            let mut hours = 0.0;
            let mut battery_wh = 0.0;
            let mut end = start;
            while end < steps.len() && inside(end) {
                let step = steps[end];
                energy += watts * step.hours;
                hours += step.hours;
                battery_wh += (watts - (step.surplus_w - extra[end]).max(0.0)).max(0.0) * step.hours;
                let full = energy >= need_wh - 1.0;
                let long_enough = hours >= min_hours - 1e-6 || need.running_w.is_some();
                if full || (need.partial && long_enough) {
                    for i in start..=end {
                        trial[i] = extra[i] + watts;
                    }
                    let candidate = Candidate { start, end, watts, need_wh, delivered_wh: energy.min(need_wh), battery_wh };
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
    let best = match best {
        Some(best) if !need.required || best.delivered_wh >= best.need_wh - 1.0 => best,
        _ if need.required => return place_topped_up(sun, steps, extra, limits, need),
        _ => return None,
    };
    for value in &mut extra[best.start..=best.end] {
        *value += best.watts;
    }
    Some(Slot {
        kind: need.kind,
        start: steps[best.start].start,
        end: steps[best.end].end(),
        watts: best.watts,
        partial: best.delivered_wh < best.need_wh - 1.0,
        topped_up: false,
    })
}

/// A load that runs today whatever the sun: its whole need where it takes the
/// least from the grid, earliest on a tie.
fn place_topped_up(sun: &Sun, steps: &[Step], extra: &mut [f64], limits: &Limits, need: &Need) -> Option<Slot> {
    let inside = |i: usize| steps[i].start < need.until && steps[i].end() > need.from;
    let starts: Vec<usize> = match need.running_w {
        Some(_) => (0..steps.len().min(1)).filter(|i| inside(*i)).collect(),
        None => (0..steps.len()).filter(|i| inside(*i)).collect(),
    };
    let levels: Vec<f64> = match need.running_w {
        Some(running) => need.levels.iter().copied().filter(|w| *w <= running + 1.0).collect(),
        None => need.levels.clone(),
    };
    let mut best: Option<(usize, usize, f64, f64, f64)> = None;
    let mut trial = extra.to_vec();
    for &start in &starts {
        for &watts in &levels {
            let mut energy = 0.0;
            let mut end = start;
            while end + 1 < steps.len() && inside(end + 1) && energy + watts * steps[end].hours < need.need_wh - 1.0 {
                energy += watts * steps[end].hours;
                end += 1;
            }
            energy += watts * steps[end].hours;
            let delivered = energy.min(need.need_wh);
            for i in start..=end {
                trial[i] = extra[i] + watts;
            }
            let grid_wh = simulate_hybrid(sun, steps, &trial, limits.low).grid_wh;
            trial[start..=end].copy_from_slice(&extra[start..=end]);
            let better = best.is_none_or(|(b_start, _, _, b_delivered, b_grid)| {
                if (delivered - b_delivered).abs() > 1.0 {
                    delivered > b_delivered
                } else if (grid_wh - b_grid).abs() > 1.0 {
                    grid_wh < b_grid
                } else {
                    start < b_start
                }
            });
            if better {
                best = Some((start, end, watts, delivered, grid_wh));
            }
        }
    }
    let (start, end, watts, delivered, grid_wh) = best?;
    for value in &mut extra[start..=end] {
        *value += watts;
    }
    Some(Slot {
        kind: need.kind,
        start: steps[start].start,
        end: steps[end].end(),
        watts,
        partial: delivered < need.need_wh - 1.0,
        topped_up: grid_wh > 1.0,
    })
}

struct Plan {
    slots: Vec<Slot>,
    sun_only_soc: f64,
    end_soc: f64,
    full_at: Option<NaiveDateTime>,
    spare_wh: f64,
    spare_before_ev_wh: f64,
    grid_wh: f64,
    grid_now: bool,
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
    let low = sun.day_low.max(sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc);
    let limits = Limits { target: baseline.end_soc, low };
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
    let run = simulate(sun, &steps, &extra);
    let hybrid = simulate_hybrid(sun, &steps, &extra, low);
    let topped_up = slots.iter().any(|slot| slot.topped_up);
    Some(Plan {
        sun_only_soc: baseline.end_soc,
        end_soc: if topped_up { hybrid.end_soc } else { run.end_soc },
        full_at: if topped_up { hybrid.full_at } else { run.full_at },
        spare_wh: spare(&extra),
        spare_before_ev_wh: spare_before_ev_wh.unwrap_or_else(|| spare(&extra)),
        grid_wh: hybrid.grid_wh,
        grid_now: hybrid.grid_now,
        slots,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The change in the house load this stands for.
    fn step(&self) -> f64 {
        match *self {
            Change::Started(_, watts) => watts,
            Change::Stopped(_, watts) => -watts,
            Change::Level(_, from, to) => to - from,
            Change::Lost(_) => 0.0,
        }
    }

    fn kind(&self) -> LoadKind {
        match self {
            Change::Started(kind, _) | Change::Stopped(kind, _) | Change::Level(kind, _, _) | Change::Lost(kind) => *kind,
        }
    }
}

/// A step in the house load that no load the app asked for explains, set
/// aside while it may be the hotplate.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub delta: f64,
    pub before_w: f64,
    pub change: Option<Change>,
}

/// Works out which load switched on or off from steps in the house load.
#[derive(Debug, Default)]
pub struct Detector {
    last: Option<(NaiveDateTime, f64)>,
    running: BTreeMap<LoadKind, Running>,
    /// A load that only stops on its own rarely (the ESP32 geyser between
    /// presses): a drop goes to another running load when it could be one.
    sticky: Option<LoadKind>,
    /// A rise no load explained (a kettle, an iron): the drop that matches it
    /// is that appliance going off, not one of the loads.
    unplaced: Option<(NaiveDateTime, f64)>,
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
        self.observe_holding(now, load_w, options, expected, |_| false).0
    }

    /// Like `observe`, but a step the app didn't ask for is handed back
    /// instead of applied whenever `hold` says so.
    pub fn observe_holding(
        &mut self,
        now: NaiveDateTime,
        load_w: f64,
        options: &[(LoadKind, Vec<f64>)],
        expected: &[LoadKind],
        hold: impl Fn(f64) -> bool,
    ) -> (Vec<Change>, Option<Held>) {
        self.running.retain(|kind, _| options.iter().any(|(option, _)| option == kind));
        let previous = self.last.replace((now, load_w));
        let mut changes = Vec::new();
        let mut held = None;
        if let Some((at, before)) = previous {
            let delta = load_w - before;
            if now - at <= Duration::minutes(MAX_GAP_MINUTES) && delta.abs() >= MIN_STEP_W {
                let change = self.best_match(delta, options, expected);
                let asked = change.as_ref().is_some_and(|change| expected.contains(&change.kind()));
                let unplaced = self
                    .unplaced
                    .filter(|(since, rise)| delta < 0.0 && now - *since <= Duration::minutes(UNPLACED_MINUTES) && (delta + rise).abs() <= tolerance(*rise))
                    .filter(|(_, rise)| change.as_ref().is_none_or(|change| (delta + rise).abs() <= (delta - change.step()).abs()));
                if unplaced.is_some() {
                    self.unplaced = None;
                } else if !asked && hold(delta) {
                    held = Some(Held { delta, before_w: before, change });
                } else if let Some(change) = change {
                    self.apply(&change, now);
                    changes.push(change);
                } else if delta > 0.0 {
                    self.unplaced = Some((now, delta));
                }
            }
        }
        while self.running_w() > load_w + LOST_MARGIN_W {
            let Some(kind) = self.running.iter().max_by_key(|(_, r)| r.since).map(|(kind, _)| *kind) else {
                break;
            };
            let from = self.running[&kind].watts;
            let room = load_w - (self.running_w() - from);
            let lower = options
                .iter()
                .filter(|(option, _)| *option == kind)
                .flat_map(|(_, levels)| levels.iter().copied())
                .filter(|level| *level < from - 1.0 && *level <= room)
                .max_by(f64::total_cmp);
            let change = match lower {
                Some(to) => Change::Level(kind, from, to),
                None => Change::Lost(kind),
            };
            self.apply(&change, now);
            changes.push(change);
        }
        (changes, held)
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
        let best = candidates
            .into_iter()
            .filter(|(_, step)| step.signum() == delta.signum() && (delta - step).abs() <= tolerance(*step))
            .map(|(change, step)| {
                let bonus = if expected.contains(&change.kind()) { EXPECTED_BONUS_W } else { 0.0 };
                ((delta - step).abs() - bonus, change)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, change)| change);
        match best {
            Some(Change::Stopped(kind, _)) if Some(kind) == self.sticky => self
                .running
                .iter()
                .filter(|(other, running)| **other != kind && (delta + running.watts).abs() <= running.watts * STICKY_SHARE)
                .min_by(|a, b| (delta + a.1.watts).abs().total_cmp(&(delta + b.1.watts).abs()))
                .map(|(other, running)| Change::Stopped(*other, running.watts))
                .or(best),
            _ => best,
        }
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
    last_live: Option<(f64, NaiveDateTime)>,
    charge_check: Option<String>,
    check: EvCheck,
}

/// Today's automatic live read: how many tries, and when the last one was.
#[derive(Debug, Default)]
struct EvCheck {
    day: Option<NaiveDate>,
    attempts: u32,
    last_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
enum EvToday {
    Needed(String),
    NotNeeded(String),
}

/// Two live reads of the scooter's own %, the only sure sign of charging.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChargeSeen {
    from: f64,
    /// When the earlier live read was; None when it's the app's last figure,
    /// which can only show a rise, not its absence.
    from_at: Option<NaiveDateTime>,
    to: f64,
    at: NaiveDateTime,
}

#[derive(Debug, Clone)]
struct HotplateAsk {
    at: NaiveDateTime,
    base_w: f64,
    step_w: f64,
    change: Option<Change>,
}

#[derive(Debug, Clone)]
struct HotplateRun {
    since: NaiveDateTime,
    /// The house load just before cooking started.
    base_w: f64,
    /// When the app started to believe it, and whether the family said so.
    believed_at: NaiveDateTime,
    confirmed: bool,
}

impl HotplateRun {
    /// A cooking day is only recorded once the family says yes or the belief
    /// has held through a few checks, never on one guess.
    fn counts(&self, now: NaiveDateTime) -> bool {
        self.confirmed || now - self.believed_at >= Duration::minutes(HOTPLATE_CONFIRM_MINUTES)
    }
}

#[derive(Debug, Default)]
struct HotplateMemory {
    asking: Option<HotplateAsk>,
    running: Option<HotplateRun>,
    quiet_since: Option<NaiveDateTime>,
    declined_at: Option<NaiveDateTime>,
    weak_notified: bool,
}

#[derive(Debug, Default)]
struct GeyserControl {
    pending: Option<(GeyserIntent, NaiveDateTime, u8)>,
    last_sent_at: Option<NaiveDateTime>,
    failed_on: Option<NaiveDate>,
    check: Option<GeyserCheck>,
    /// Elements left selected when the thermostat cut the geyser out; the
    /// app switches them off so it doesn't heat again on its own.
    idle_elements: Option<(bool, bool)>,
    watch: Option<GeyserWatch>,
    night_hold: bool,
    reading_at: Option<NaiveDateTime>,
    last_load_w: Option<f64>,
    /// Load beyond the usual house, the motor, the scooter and the geyser
    /// itself. The hotplate's own estimate isn't trusted here: it takes up
    /// whatever is left, so a geyser on it hasn't seen would hide inside it.
    unknown_w: f64,
    /// The live battery has been draining with the geyser on since then.
    drain_since: Option<NaiveDateTime>,
    drain_w: f64,
    /// What the geyser may draw now without overloading the inverter.
    room_w: f64,
    /// The geyser must come down now for the inverter's sake: no waiting.
    urgent: bool,
    stepped_down_at: Option<NaiveDateTime>,
    readings_seen: u32,
    /// The family said the load the app couldn't place isn't the hotplate.
    not_hotplate: Option<(NaiveDateTime, f64)>,
    /// After pressing an idle geyser's elements off: the load then and the
    /// level it ran at. A drop of that size next means it was still heating.
    idle_check: Option<(f64, f64)>,
    asked_unknown_at: Option<NaiveDateTime>,
    /// The evening reheat: when the app switched the hot geyser back on, and
    /// when it ended (the thermostat cut out again, or nothing drew).
    reheat_since: Option<NaiveDateTime>,
    reheated_at: Option<NaiveDateTime>,
    /// The load and the buttons when the app last pressed to switch it on.
    on_sent: Option<(f64, Vec<Button>)>,
}

/// The first minutes of a geyser run, watched before the plan may change it.
#[derive(Debug, Clone)]
struct GeyserWatch {
    since: NaiveDateTime,
    watts: f64,
    started_at: NaiveDateTime,
    until: NaiveDateTime,
    ended_at: Option<NaiveDateTime>,
    summary: Option<String>,
    announced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Full,
    OffOnly,
    VerifyOnly,
}

impl GeyserControl {
    fn checked(&mut self, now: NaiveDateTime, state: CheckState, note: String) {
        if let Some(check) = self.check.as_mut() {
            check.state = state;
            check.checked_at = Some(now);
            check.note = note;
        }
    }
}

fn intent_text(intent: GeyserIntent) -> String {
    match intent {
        GeyserIntent::On(watts) => format!("On at {watts:.0} W"),
        GeyserIntent::Off => "Off".into(),
    }
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
    hotplate: HotplateMemory,
    ai: ai::AiMemory,
    /// When the motor was first seen on in its current run.
    water_seen_since: Option<NaiveDateTime>,
    water_ask: Option<NaiveDateTime>,
    grid_now: Option<String>,
    elements: ElementsBelief,
}

/// Which of the geyser's two elements the app believes are switched on. They
/// stay selected when the thermostat cuts out, so this outlives the day.
#[derive(Debug, Default, Clone)]
struct ElementsBelief {
    selected: Option<(bool, bool)>,
    at: Option<NaiveDateTime>,
    why: String,
    unsure: bool,
}

impl ElementsBelief {
    fn set(&mut self, selected: (bool, bool), now: NaiveDateTime, why: String) {
        *self = ElementsBelief { selected: Some(selected), at: Some(now), why, unsure: false };
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ElementState {
    Heating,
    /// Switched on but the thermostat has cut out: the panel light blinks.
    KeepWarm,
    Off,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GeyserElements {
    pub low: ElementState,
    pub high: ElementState,
    pub unsure: bool,
    pub why: String,
    pub at: Option<NaiveDateTime>,
}

/// What the app believes about the loads, kept across a restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLoads {
    pub at: NaiveDateTime,
    last: Option<(NaiveDateTime, f64)>,
    running: BTreeMap<LoadKind, Running>,
    geyser_delivered_wh: f64,
    geyser_heated_at: Option<NaiveDateTime>,
    #[serde(default)]
    elements: Option<(bool, bool)>,
}

impl SavedLoads {
    /// Whether anything worth saving changed since `other`.
    pub fn differs(&self, other: &SavedLoads) -> bool {
        self.running != other.running
            || self.geyser_heated_at != other.geyser_heated_at
            || self.elements != other.elements
            || (self.geyser_delivered_wh - other.geyser_delivered_wh).abs() >= 50.0
            || self.at - other.at >= Duration::minutes(5)
    }
}

impl LoadsMemory {
    pub fn saved(&self, now: NaiveDateTime) -> SavedLoads {
        SavedLoads {
            at: now,
            last: self.detector.last,
            running: self.detector.running.clone(),
            geyser_delivered_wh: self.geyser_delivered_wh,
            geyser_heated_at: self.geyser_heated_at,
            elements: self.elements.selected,
        }
    }

    /// Picks up what was on before a restart, if it was saved recently today.
    pub fn restore(&mut self, saved: SavedLoads, now: NaiveDateTime) -> bool {
        if saved.at.date() != now.date() || now - saved.at > Duration::minutes(SAVED_LOADS_FRESH_MINUTES) || now < saved.at {
            return false;
        }
        self.day = Some(now.date());
        self.last_at = Some(saved.at);
        self.detector.last = saved.last;
        self.detector.running = saved.running;
        self.geyser_delivered_wh = saved.geyser_delivered_wh;
        self.geyser_heated_at = saved.geyser_heated_at;
        if let Some(selected) = saved.elements {
            self.elements.set(selected, saved.at, "remembered from before the app restarted".into());
        }
        true
    }

    /// A solar-only load on now, or one the family has just been told to switch on.
    pub fn sun_only_load(&self) -> Option<&'static str> {
        let ev_on = self.detector.running(LoadKind::Ev).is_some();
        let ev_ready = self.rows.get(&LoadKind::Ev).is_some_and(|row| row.phase == LoadPhase::Ready);
        (ev_on || ev_ready).then_some("the scooter")
    }

    /// The loads on now that need the grid, as "the geyser and the scooter".
    pub fn grid_now(&self) -> Option<&str> {
        self.grid_now.as_deref()
    }

    #[cfg(test)]
    pub fn set_grid_now(&mut self, loads: Option<&str>) {
        self.grid_now = loads.map(str::to_string);
    }

    pub fn geyser_sent(&mut self, command: &GeyserCommand, now: NaiveDateTime, ok: bool) {
        self.geyser.last_sent_at = Some(now);
        if ok {
            let pressed = (command.buttons.contains(&Button::W800), command.buttons.contains(&Button::W1200));
            let after = match (self.elements.selected, command.intent) {
                (Some((low, high)), _) => (low ^ pressed.0, high ^ pressed.1),
                (None, GeyserIntent::On(_)) => pressed,
                (None, GeyserIntent::Off) => (false, false),
            };
            let presses: Vec<&str> = command.buttons.iter().map(|button| button.label()).collect();
            self.elements.set(after, now, format!("the app pressed {} at {}", presses.join(" and "), clock_time(now)));
        }
        if ok {
            let attempts = match self.geyser.pending {
                Some((intent, _, attempts)) if intent == command.intent => attempts + 1,
                _ => 1,
            };
            self.geyser.pending = Some((command.intent, now, attempts));
            self.geyser.on_sent = match command.intent {
                GeyserIntent::On(_) => self.geyser.last_load_w.map(|load| (load, command.buttons.clone())),
                GeyserIntent::Off => None,
            };
        } else {
            self.geyser.failed_on = Some(now.date());
            self.geyser.pending = None;
        }
        let presses: Vec<&str> = command.buttons.iter().map(|button| button.label()).collect();
        self.geyser.check = Some(GeyserCheck {
            action: intent_text(command.intent),
            sent_at: now,
            state: if ok { CheckState::Checking } else { CheckState::NotConfirmed },
            checked_at: (!ok).then_some(now),
            note: if ok {
                format!("Pressed {}; checking the load.", presses.join(", then "))
            } else {
                "The ESP32 didn't answer, so nothing was pressed.".into()
            },
        });
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
        let (night_hold, readings_seen) = (self.geyser.night_hold, self.geyser.readings_seen);
        self.geyser = GeyserControl { night_hold, readings_seen, ..GeyserControl::default() };
        self.hotplate = HotplateMemory::default();
        self.ai = ai::AiMemory::default();
        self.ev.check = EvCheck::default();
        self.water_seen_since = None;
        self.water_ask = None;
    }

    fn expected(&self, now: NaiveDateTime) -> Vec<LoadKind> {
        let mut kinds: Vec<LoadKind> = self
            .rows
            .iter()
            .filter(|(_, row)| row.phase == LoadPhase::Ready || row.ready_at.is_some_and(|at| now - at <= Duration::minutes(EXPECT_MINUTES)))
            .map(|(kind, _)| *kind)
            .collect();
        if self.geyser.pending.is_some() {
            kinds.push(LoadKind::Geyser);
        }
        kinds
    }

    fn update_ev_soc(&mut self, reading: Option<&EvReading>, charging: bool, now: NaiveDateTime, config: &AutomationConfig) -> Option<ChargeSeen> {
        if let Some(reading) = reading.filter(|r| self.ev.last_reading_at.is_none_or(|at| r.at > at)) {
            self.ev.last_reading_at = Some(reading.at);
            let news = reading.live || self.ev.soc.is_none() || self.ev.last_passive_soc != Some(reading.soc);
            let mut seen = None;
            if reading.live {
                let earlier_live = self.ev.last_live.filter(|(_, at)| reading.at - *at <= Duration::hours(EV_COMPARE_HOURS));
                seen = match earlier_live {
                    Some((from, from_at)) => Some(ChargeSeen { from, from_at: Some(from_at), to: reading.soc, at: reading.at }),
                    None => self
                        .ev
                        .soc
                        .as_ref()
                        .filter(|known| reading.soc - known.soc >= EV_RISE_PCT)
                        .map(|known| ChargeSeen { from: known.soc, from_at: None, to: reading.soc, at: reading.at }),
                };
                if seen.is_none() {
                    self.ev.charge_check = Some(format!("Read live: {:.0}% at {}.", reading.soc, clock_time(reading.at)));
                }
                self.ev.last_live = Some((reading.soc, reading.at));
            } else {
                self.ev.last_passive_soc = Some(reading.soc);
            }
            if news && (reading.live || !charging || self.ev.soc.is_none()) {
                let basis = if reading.live { SocBasis::Live } else { SocBasis::Read };
                self.ev.soc = Some(EvSoc { soc: reading.soc, at: now, basis });
                return seen;
            }
        }
        if let Some(soc) = self.ev.soc.as_mut().filter(|_| charging) {
            let hours = (now - soc.at).num_seconds().max(0) as f64 / 3600.0;
            soc.soc = (soc.soc + ev_rate_pct_per_hour(config) * hours).min(100.0);
            soc.basis = if soc.soc >= 100.0 { SocBasis::Full } else { SocBasis::Estimated };
            soc.at = now;
            if soc.soc >= 100.0 {
                self.detector.running.remove(&LoadKind::Ev);
            }
        }
        None
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

/// The family's rule: at or below the threshold it charges today; above it,
/// only when tomorrow's sun can't charge it and today's can.
fn ev_today(soc: f64, config: &AutomationConfig, tomorrow: Option<&EvDayRow>, today_sunny: bool, geyser_kwh: f64) -> EvToday {
    let limit = config.ev_no_charge_above_pct;
    if soc <= limit {
        return EvToday::Needed(format!("{soc:.0}%, at or below {limit:.0}%"));
    }
    if soc >= EV_NEARLY_FULL_PCT {
        return EvToday::NotNeeded(format!("{soc:.0}%, already full enough"));
    }
    let tomorrow_need_kwh = ev_need_wh((soc - config.ev_daily_use_pct).max(0.0), config) / 1000.0;
    let tomorrow_short = tomorrow.and_then(|day| day.expected_spare_kwh).is_some_and(|spare| spare - geyser_kwh < tomorrow_need_kwh);
    match (tomorrow_short, today_sunny) {
        (true, true) => EvToday::Needed(format!("{soc:.0}%, but tomorrow looks too cloudy to charge it and today is sunny")),
        (true, false) => EvToday::NotNeeded(format!("{soc:.0}%; tomorrow looks cloudy but today's sun can't charge it either")),
        _ => EvToday::NotNeeded(format!("{soc:.0}%, and tomorrow looks fine")),
    }
}

fn slot_wh(slots: &[Slot], kind: LoadKind) -> f64 {
    slots.iter().filter(|slot| slot.kind == kind).map(|slot| slot.watts * (slot.end - slot.start).num_seconds() as f64 / 3600.0).sum()
}

/// Today's 15-minute steps as hours for the spare-sun chart. A load on now
/// counts in the first hour, and in later ones while its run lasts.
fn sun_hours(steps: &[Step], running: &[(LoadKind, f64)], slots: &[Slot]) -> Vec<SunHour> {
    let mut groups: Vec<(NaiveDateTime, f64, f64, f64)> = Vec::new();
    for step in steps {
        match groups.last_mut() {
            Some(group) if group.0.date() == step.start.date() && group.0.hour() == step.start.hour() => {
                group.1 += step.hours;
                group.2 += step.pv_w * step.hours;
                group.3 += step.house_w * step.hours;
            }
            _ => groups.push((step.start, step.hours, step.pv_w * step.hours, step.house_w * step.hours)),
        }
    }
    groups
        .iter()
        .enumerate()
        .filter(|(_, (_, hours, _, _))| *hours > 0.0)
        .map(|(index, (at, hours, pv, house))| SunHour {
            at: *at,
            sun_w: (pv / hours).round(),
            house_w: (house / hours).round(),
            loads_w: running
                .iter()
                .filter(|(kind, _)| index == 0 || slots.iter().any(|slot| slot.kind == *kind && slot.end > *at))
                .map(|(_, watts)| watts.round())
                .sum(),
        })
        .collect()
}

fn fallback_ev_charge(soc: f64, config: &AutomationConfig, today_spare_wh: Option<f64>, tomorrow_spare_kwh: Option<f64>, geyser_kwh: f64) -> EvCharge {
    let need_kwh = ev_need_wh(soc, config) / 1000.0 + geyser_kwh;
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

/// How many watts the geyser may draw without the inverter overloading:
/// everything else on now stays. While the hotplate may be on the geyser
/// gets its lowest level only, off above HOTPLATE_GEYSER_OFF_W and back on
/// below HOTPLATE_GEYSER_ON_W.
fn geyser_room(
    input: &LoadsInput,
    geyser: &Option<Running>,
    water: &Option<Running>,
    ev: &Option<Running>,
    hotplate_maybe_on: bool,
    config: &AutomationConfig,
) -> f64 {
    if hotplate_maybe_on {
        let low = config.geyser_low_watts;
        let Some(load) = input.load_w else { return 0.0 };
        let others = load - geyser.as_ref().map_or(0.0, |r| r.watts);
        let limit = if geyser.is_some() { HOTPLATE_GEYSER_OFF_W } else { HOTPLATE_GEYSER_ON_W };
        return if others + low <= limit { low } else { 0.0 };
    }
    let budget = config.inverter_max_w * INVERTER_BUDGET;
    let others = match input.load_w {
        Some(load) => load - geyser.as_ref().map_or(0.0, |r| r.watts),
        None => [water, ev].iter().filter_map(|r| r.as_ref().map(|r| r.watts)).sum::<f64>() + (input.typical_load)(input.now.hour()).unwrap_or(0.0),
    };
    (budget - others).max(0.0)
}

/// The geyser comes down at once, without waiting on the AI, when it would
/// leave the inverter too little room.
fn keep_inverter_safe(running: &Running, room: f64, hotplate: bool, config: &AutomationConfig) -> Verdict {
    let level = geyser_levels(config).into_iter().find(|level| *level <= room && *level < running.watts - 1.0);
    let note = match (hotplate, level) {
        (true, Some(level)) => format!(
            "A big load is on (the motor or the hotplate): geyser down to {} so the inverter isn't overloaded. Keep the hotplate at {HOTPLATE_KEEP_BELOW_W:.0} W or below.",
            level_text(level, config)
        ),
        (true, None) => format!(
            "The house is near {HOTPLATE_GEYSER_OFF_W:.0} W with a big load on: geyser off. It comes back on at {} when there's room.",
            level_text(config.geyser_low_watts, config)
        ),
        (false, Some(level)) => format!("The house is near the inverter's limit: geyser down to {}.", level_text(level, config)),
        (false, None) => "The house is near the inverter's limit: geyser off.".to_string(),
    };
    Verdict { phase: LoadPhase::Stop, note, watts: level, slot: None }
}

/// The battery is draining with the geyser on: the geyser gives way first,
/// down to the level the sun can still carry, or off.
fn turn_down_for_drain(running: &Running, drain: f64, config: &AutomationConfig) -> Verdict {
    let fits = running.watts - drain;
    match geyser_levels(config).into_iter().find(|level| *level <= fits && *level < running.watts - 1.0) {
        Some(level) => Verdict {
            phase: LoadPhase::Stop,
            note: format!("The battery is draining {drain:.0} W: geyser down to {} so the sun carries it.", level_text(level, config)),
            watts: Some(level),
            slot: None,
        },
        None => Verdict {
            phase: LoadPhase::Stop,
            note: format!("The battery is draining {drain:.0} W and even the lowest level wouldn't fit: geyser off."),
            watts: None,
            slot: None,
        },
    }
}

/// With the battery full and not draining, the sun is carrying everything
/// that is on, so nothing running is told to come off or go down, and a
/// load that is due doesn't need the grid.
fn sun_carries(verdict: Verdict, running_w: Option<f64>, carried: bool) -> Verdict {
    if !carried {
        return verdict;
    }
    match (verdict.phase, running_w) {
        (LoadPhase::Stop, Some(watts)) => Verdict {
            phase: LoadPhase::Running,
            note: "The battery is full and not draining: the sun is carrying it.".into(),
            watts: Some(watts),
            slot: verdict.slot,
        },
        (LoadPhase::Ready, None) if verdict.slot.as_ref().is_some_and(|slot| slot.topped_up) => Verdict {
            note: "The battery is full, so the sun has more to give than the panels show: switch it on now.".into(),
            ..verdict
        },
        _ => verdict,
    }
}

/// A load already on while the battery still charges: the sun carries both,
/// so it isn't switched off as long as the battery still fills today.
/// Without a battery reading there's nothing to judge on, so a load already
/// on is left alone rather than switched off on a guess.
fn sun_covers(verdict: Verdict, running_w: Option<f64>, full_at: Option<NaiveDateTime>, no_reading: bool) -> Verdict {
    match (verdict.phase, running_w, full_at) {
        (LoadPhase::Stop, Some(watts), Some(full_at)) if verdict.watts.is_none() => Verdict {
            phase: LoadPhase::Running,
            note: format!("On the sun: the battery is still charging and fills by about {}.", clock_time(full_at)),
            watts: Some(watts),
            slot: verdict.slot,
        },
        (LoadPhase::Stop, Some(watts), None) if no_reading && verdict.watts.is_none() => Verdict {
            phase: LoadPhase::Running,
            note: "No battery reading just now: leaving it on until the next one.".into(),
            watts: Some(watts),
            slot: verdict.slot,
        },
        _ => verdict,
    }
}

struct SunNow {
    /// Sun now beyond the usual house and the other loads that are running.
    spare: f64,
    full: bool,
    draining: bool,
    /// The house load is what the app knows about: no big load it can't place,
    /// such as a geyser switched on by hand that it hasn't seen yet.
    explained: bool,
    /// Long enough since the last press, and since the last step down, to try a level up.
    step_up_ok: bool,
}

/// The ESP32 geyser takes sun that nothing running is using: with the battery
/// full it comes on at the biggest level the spare sun covers instead of
/// waiting for loads the family hasn't switched on, and it stays on while the
/// sun carries it and the battery isn't draining.
fn sun_for_geyser(verdict: Verdict, sun: &SunNow, running: Option<&Running>, config: &AutomationConfig) -> Verdict {
    let reach = sun.spare + GEYSER_LEND_W;
    match running {
        None if sun.full && sun.explained && matches!(verdict.phase, LoadPhase::Waiting | LoadPhase::Skipped | LoadPhase::Ready) => {
            match geyser_levels(config).into_iter().find(|level| *level <= reach) {
                Some(level) if verdict.phase != LoadPhase::Ready || verdict.watts.is_none_or(|watts| level > watts + 1.0) => Verdict {
                    phase: LoadPhase::Ready,
                    note: format!(
                        "The battery is full and the sun gives about {:.0} W spare: geyser on at {}, a level down if the battery starts draining.",
                        sun.spare,
                        level_text(level, config)
                    ),
                    watts: Some(level),
                    slot: None,
                },
                _ => verdict,
            }
        }
        Some(running) if verdict.phase == LoadPhase::Running && sun.full && !sun.draining && sun.step_up_ok => {
            let up = geyser_levels(config).into_iter().filter(|level| *level > running.watts + 1.0 && *level <= reach).reduce(f64::min);
            match up {
                Some(level) => Verdict {
                    phase: LoadPhase::Running,
                    note: format!("The battery is full and not draining: geyser up to {}.", level_text(level, config)),
                    watts: Some(level),
                    slot: None,
                },
                None => verdict,
            }
        }
        Some(running) if verdict.phase == LoadPhase::Stop && !sun.draining && running.watts <= sun.spare + GEYSER_SUN_DRAIN_W => Verdict {
            phase: LoadPhase::Running,
            note: format!("The sun carries it at {} and the battery isn't draining.", level_text(running.watts, config)),
            watts: Some(running.watts),
            slot: None,
        },
        _ => verdict,
    }
}

/// The hot geyser's one evening top-up: on at the biggest level the spare sun
/// alone covers, only with the battery full and nothing big on.
fn reheat_verdict(sun: &SunNow, heated_at: NaiveDateTime, busy: bool, config: &AutomationConfig) -> Verdict {
    let level = geyser_levels(config).into_iter().find(|level| *level <= sun.spare);
    match level {
        Some(level) if sun.full && sun.explained && !sun.draining && !busy => Verdict {
            phase: LoadPhase::Ready,
            note: format!(
                "Hot since {}, cooling since: topping it up at {} on about {:.0} W of spare sun so the water lasts till morning.",
                clock_time(heated_at),
                level_text(level, config),
                sun.spare
            ),
            watts: Some(level),
            slot: None,
        },
        _ => Verdict {
            phase: LoadPhase::Waiting,
            note: format!(
                "Hot since {}. Tops it up once more when the battery is full, nothing big is on and the spare sun covers {}.",
                clock_time(heated_at),
                level_text(config.geyser_low_watts.min(config.geyser_high_watts), config)
            ),
            watts: None,
            slot: None,
        },
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
        let repeat = if repeat > Duration::zero() { repeat } else { self.repeat };
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

fn next_days(input: &LoadsInput, house_kwh: f64, battery_kwh: f64, cooking_anchor: Option<NaiveDate>) -> Vec<EvDayRow> {
    let Some(forecast) = input.forecast else {
        return Vec::new();
    };
    let config = input.config;
    let today = input.now.date();
    let water_kwh = if config.water_reminder_enabled { config.water_motor_watts * config.water_run_minutes as f64 / 60_000.0 } else { 0.0 };
    let every = i64::from(config.hotplate_every_days);
    let cooking = |date: NaiveDate| cooking_anchor.is_some_and(|anchor| date > anchor && (date - anchor).num_days() % every == 0);
    forecast
        .daily
        .iter()
        .filter(|day| day.date > today && day.date <= today + Duration::days(3))
        .map(|day| {
            let pv = day
                .radiation_kwh_m2
                .filter(|_| config.pv_array_watts > 0.0)
                .map(|kwh_m2| kwh_m2 * config.pv_array_watts / 1000.0 * input.pv_ratio);
            let cooking_day = cooking(day.date);
            let hotplate_kwh = if cooking_day { config.hotplate_watts * config.hotplate_hours / 1000.0 } else { 0.0 };
            EvDayRow {
                date: day.date.format("%a %d %b").to_string(),
                relative: if day.date == today + Duration::days(1) { "tomorrow".into() } else { day.date.format("%A").to_string() },
                radiation_kwh_m2: day.radiation_kwh_m2.map(|v| (v * 10.0).round() / 10.0),
                sunshine_h: day.sunshine_h.map(|v| (v * 10.0).round() / 10.0),
                cloud_pct: forecast_cloud(forecast, day.date),
                rain_prob_pct: day.rain_prob_pct,
                expected_pv_kwh: pv.map(|v| (v * 10.0).round() / 10.0),
                cooking_day,
                expected_spare_kwh: pv.map(|v| ((v - house_kwh - battery_kwh - water_kwh - hotplate_kwh).max(0.0) * 10.0).round() / 10.0),
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
    let esp32 = config.geyser_auto_control && !config.geyser_esp32_host.is_empty();
    let pressed_lately = |at: Option<NaiveDateTime>| at.is_some_and(|at| now - at <= Duration::minutes(GEYSER_FREE_MINUTES));
    let geyser_free = !esp32
        || memory.geyser.pending.is_some()
        || pressed_lately(memory.geyser.last_sent_at)
        || pressed_lately(input.geyser_heard_at)
        || input.edits.iter().any(|edit| edit.load == LoadKind::Geyser && edit.state == EditState::On)
        || memory.pinned_on(LoadKind::Geyser, now, input.load_w);
    let options: Vec<(LoadKind, Vec<f64>)> = load_options(config)
        .into_iter()
        .filter_map(|(kind, levels)| match kind {
            LoadKind::Geyser if !geyser_free && memory.detector.running(kind).is_some() => Some((kind, Vec::new())),
            LoadKind::Geyser if !geyser_free => None,
            _ => Some((kind, levels)),
        })
        .collect();
    let mut infos: Vec<(&'static str, String)> = Vec::new();

    match input.answer {
        Some(LoadsAnswer::HotplateDone) => end_hotplate(memory),
        Some(answer) if memory.hotplate.asking.is_some() => {
            let asked_w = memory.hotplate.asking.as_ref().map(|ask| ask.step_w);
            answer_hotplate(memory, answer == LoadsAnswer::Yes, now, (input.typical_load)(now.hour()).unwrap_or(0.0));
            if answer == LoadsAnswer::No {
                memory.geyser.not_hotplate = asked_w.map(|watts| (now, watts));
            }
        }
        Some(answer) => memory.answer_ai(answer == LoadsAnswer::Yes, now),
        None => {}
    }
    for edit in input.edits {
        memory.apply_edit(edit, now, input.load_w, config);
        match (edit.load, edit.state) {
            (LoadKind::Geyser, EditState::On) => {
                if let Some(selected) = edit.watts.and_then(|watts| geyser_elements(watts, config)) {
                    memory.elements.set(selected, now, format!("you said it's on at {:.0} W", edit.watts.unwrap_or_default()));
                }
            }
            (LoadKind::Geyser, EditState::Off) => memory.elements.set((false, false), now, "you said it's off".into()),
            _ => {}
        }
    }
    let cooked_today = input.hotplate_on == Some(today);
    let hotplate_skipped = input.hotplate_skip_on == Some(today);
    let in_hotplate_window =
        input.daytime && now >= at_hour(today, config.hotplate_start_hour) && now < at_hour(today, config.hotplate_end_hour);
    let holding = memory.hotplate.running.is_some() || memory.hotplate.asking.is_some();
    let may_ask = config.hotplate_enabled
        && !input.ai_available
        && in_hotplate_window
        && !hotplate_skipped
        && !cooked_today
        && memory.hotplate.declined_at.is_none_or(|at| now - at >= Duration::minutes(HOTPLATE_ASK_GAP_MINUTES));
    let fresh_reading = input.load_w.is_some() && input.load_w != memory.geyser.last_load_w;
    if let Some((then, watts)) = memory.geyser.idle_check.filter(|_| fresh_reading) {
        memory.geyser.idle_check = None;
        let load = input.load_w.unwrap_or(then);
        if then - load >= watts * IDLE_CHECK_SHARE {
            tracing::info!("loads: the geyser was still heating when taken as hot ({then:.0} → {load:.0} W) — not hot after all");
            memory.geyser_heated_at = None;
            memory.detector.last = Some((now, load));
            infos.push((
                GEYSER_TITLE,
                format!(
                    "The load fell by {:.0} W when the app switched the geyser's elements off, so it was still heating, not hot. Another load had stopped. The app will switch it back on.",
                    then - load
                ),
            ));
        }
    }
    memory.detector.sticky = (!geyser_free).then_some(LoadKind::Geyser);
    let running_before_w = memory.detector.running_w();
    let (changes, held) = match input.load_w {
        Some(load) => memory.detector.observe_holding(now, load, &options, &expected, |delta| holding || (may_ask && delta > 0.0)),
        None => (Vec::new(), None),
    };
    if let Some(held) = held.filter(|_| !holding) {
        tracing::info!("loads: unasked step of {:.0} W at {} — asking about the hotplate", held.delta, clock_time(now));
        memory.hotplate.asking = Some(HotplateAsk {
            at: now,
            base_w: (held.before_w - running_before_w).max(0.0),
            step_w: held.delta,
            change: held.change,
        });
        result.asks.push(Ask {
            title: HOTPLATE_TITLE,
            body: format!("A {:.1} kW load came on at {} — is it the hotplate?", held.delta / 1000.0, clock_time(now)),
            about: AskAbout::Loads,
        });
    }
    let asked_off = |memory: &LoadsMemory, kind: LoadKind| memory.rows.get(&kind).is_some_and(|row| row.phase == LoadPhase::Stop);
    let (geyser_asked_off, ev_asked_off) = (asked_off(memory, LoadKind::Geyser), asked_off(memory, LoadKind::Ev));
    for change in &changes {
        tracing::info!("loads: {change:?} at {}", clock_time(now));
        match change {
            Change::Stopped(LoadKind::Geyser, watts) if !geyser_asked_off && memory.geyser.pending.is_none() => {
                let enough = memory.geyser_delivered_wh >= (config.geyser_daily_kwh * 1000.0 * GEYSER_HOT_SHARE).max(GEYSER_HOT_MIN_WH);
                if memory.geyser_heated_at.is_none() && input.daytime && !enough {
                    infos.push((
                        GEYSER_TITLE,
                        format!(
                            "The geyser went off at {} after only {:.1} kWh today: too little to have heated the tank, so taken as switched off by hand.",
                            clock_time(now),
                            memory.geyser_delivered_wh / 1000.0
                        ),
                    ));
                } else if memory.geyser_heated_at.is_none() && input.daytime {
                    memory.geyser_heated_at = Some(now);
                    memory.geyser.idle_elements = geyser_elements(*watts, config);
                    infos.push((
                        GEYSER_TITLE,
                        format!(
                            "The geyser went off at {} ({:.1} kWh today): taken as its thermostat, so the water is hot. If you switched it off, tap Change on the geyser.",
                            clock_time(now),
                            memory.geyser_delivered_wh / 1000.0
                        ),
                    ));
                } else if memory.geyser.reheat_since.is_some() {
                    memory.geyser.reheat_since = None;
                    memory.geyser.reheated_at = Some(now);
                    memory.geyser.idle_elements = geyser_elements(*watts, config);
                    infos.push((
                        GEYSER_TITLE,
                        format!(
                            "Topped up: the thermostat cut the geyser out again at {}, so the tank goes into the night at its top temperature.",
                            clock_time(now)
                        ),
                    ));
                }
            }
            Change::Stopped(LoadKind::Ev, _) if !ev_asked_off => {
                let estimate = ev_was.as_ref().map(|soc| soc.soc);
                if estimate.is_some_and(|soc| soc >= EV_FULL_GUESS_SOC) {
                    memory.ev.soc = Some(EvSoc { soc: 100.0, at: now, basis: SocBasis::Full });
                    infos.push((EV_TITLE, EV_FULL_MESSAGE.into()));
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
    if let Some(seen) = memory.update_ev_soc(input.ev_reading.as_ref(), charging, now, config) {
        memory.ev_charge_seen(seen, now, input.load_w, config);
    }

    let water_running = memory.detector.running(LoadKind::Water).cloned();
    let geyser_running = memory.detector.running(LoadKind::Geyser).cloned();
    let ev_running = memory.detector.running(LoadKind::Ev).cloned();
    let flexible_w: f64 = [&water_running, &geyser_running, &ev_running].iter().filter_map(|r| r.as_ref().map(|r| r.watts)).sum();
    let hotplate_since = memory.hotplate.running.as_ref().map(|r| (r.since, r.base_w)).or(memory.hotplate.asking.as_ref().map(|a| (a.at, a.base_w)));
    let hotplate_w = match (input.load_w, hotplate_since) {
        (Some(load), Some((_, base))) => Some((load - base - flexible_w).max(0.0)),
        _ => None,
    };
    track_hotplate_off(memory, hotplate_w, now);
    let hotplate_active = memory.hotplate.running.is_some() || memory.hotplate.asking.is_some();
    let hotplate_w = hotplate_w.filter(|_| hotplate_active);

    let soc_ready = input.soc.is_some();
    let battery_full = input.soc.is_some_and(|soc| soc >= FULL_SOC);
    let nearly_full_idle = input.soc.is_some_and(|soc| soc >= NEARLY_FULL_SOC) && input.charge_a.is_none_or(|amps| amps < NEARLY_FULL_IDLE_A);
    let charge_capped = match (input.charge_a, input.max_charge_a) {
        (Some(charge), Some(max)) => charge >= max * 0.9,
        _ => false,
    };
    let held_back = battery_full || charge_capped || nearly_full_idle;
    let battery_w = input.battery_w.or(input.pv_w.zip(input.load_w).map(|(pv, load)| pv - load));
    let carried = held_back && input.daytime && now < input.sunset && battery_w.is_none_or(|watts| watts >= -GEYSER_SUN_DRAIN_W);
    let forecast_pv = |at: NaiveDateTime| {
        if let Some(watts) = solar::curve_at(input.pv_curve, at + Duration::minutes(STEP_MINUTES) / 2) {
            return Some(watts);
        }
        let forecast = input.forecast?;
        (config.pv_array_watts > 0.0)
            .then(|| forecast.radiation_during(at))
            .flatten()
            .map(|radiation| radiation * config.pv_array_watts / 1000.0 * input.pv_ratio)
    };
    let pv_now_w = match (input.pv_w, forecast_pv(now)) {
        (Some(live), Some(forecast)) if held_back => Some(live.max(forecast)),
        (Some(live), _) => Some(live),
        (None, forecast) => forecast,
    };
    let house_now_w = input.load_w.map(|load| (load - flexible_w - hotplate_w.unwrap_or(0.0)).max(0.0));

    match (&water_running, memory.water_seen_since) {
        (Some(running), None) => memory.water_seen_since = Some(running.since),
        (None, Some(since)) => {
            memory.water_seen_since = None;
            if input.water_pending && now - since >= Duration::minutes(TANK_ASK_AFTER_MINUTES) {
                memory.water_ask = Some(now);
                result.asks.push(Ask {
                    title: crate::automation::water::TITLE,
                    body: format!("The motor stopped at {}. Is the tank full?", clock_time(now)),
                    about: AskAbout::Tank,
                });
            }
        }
        _ => {}
    }
    if !input.water_pending {
        memory.water_ask = None;
    }

    let ev_soc = memory.ev.soc.as_ref().map(|soc| soc.soc);
    let decision = memory.ev.decision.as_ref().filter(|d| d.at.date() == today).cloned();
    let geyser_done = input.geyser_done_on == Some(today);
    let geyser_season = config.geyser_planned_on(today);
    let tomorrow_geyser_kwh = if config.geyser_planned_on(today + Duration::days(1)) { config.geyser_daily_kwh } else { 0.0 };
    let geyser_need = if geyser_season && !geyser_done { geyser_need_wh(memory, config, geyser_running.is_some()) } else { 0.0 };
    let ev_skipped = input.ev_skip_on == Some(today) && ev_running.is_none();
    let ai_state = format!(
        "{}{geyser_done}{ev_skipped}{hotplate_skipped}{cooked_today}{}{}",
        input.water_pending,
        memory.geyser_heated_at.is_some(),
        ev_soc.map_or(-1.0, |soc| (soc / 5.0).round())
    );
    let ev_need_full = if config.ev_enabled && !ev_skipped { ev_soc.map_or(0.0, |soc| ev_need_wh(soc, config)) } else { 0.0 };
    let water_need = if input.water_pending || water_running.is_some() {
        config.water_motor_watts * config.water_run_minutes as f64 / 60.0
    } else {
        0.0
    };

    let sunset = input.sunset;
    let geyser_until = at_hour(today, config.geyser_end_hour).min(sunset);
    let hotplate_until = at_hour(today, config.hotplate_end_hour).min(sunset);
    let scheduled = input.hotplate_on.is_none_or(|last| (today - last).num_days() >= i64::from(config.hotplate_every_days));
    let cooking_day = config.hotplate_enabled && (hotplate_active || (!hotplate_skipped && (cooked_today || scheduled)));
    let hotplate_need = if let (true, Some((since, _))) = (hotplate_active, hotplate_since) {
        let watts = hotplate_w.unwrap_or(config.hotplate_watts).max(HOTPLATE_MIN_W);
        let elapsed_h = (now - since).num_seconds().max(0) as f64 / 3600.0;
        let hours = (config.hotplate_hours - elapsed_h).max(0.25);
        Some(Need {
            kind: LoadKind::Hotplate,
            levels: hotplate_levels(watts),
            need_wh: watts * hours,
            from: now,
            until: sunset,
            running_w: Some(watts),
            partial: false,
            session_h: Some(hours),
            required: false,
        })
    } else if cooking_day && !cooked_today && now < hotplate_until {
        Some(Need {
            kind: LoadKind::Hotplate,
            levels: hotplate_levels(config.hotplate_watts),
            need_wh: config.hotplate_watts * config.hotplate_hours,
            from: at_hour(today, config.hotplate_start_hour),
            until: hotplate_until,
            running_w: None,
            partial: false,
            session_h: Some(config.hotplate_hours),
            required: false,
        })
    } else {
        None
    };
    let geyser_first = geyser_first_reason(input);
    let cold_morning = input.forecast.and_then(|forecast| forecast.day(today)).and_then(|day| day.temp_min_c);
    let geyser_required = (config.geyser_enabled && geyser_need > 0.0)
        .then(|| match cold_morning {
            _ if config.geyser_winter_only => Some("it's winter: hot water every day".to_string()),
            Some(temp) if temp <= GEYSER_REQUIRED_BELOW_C => Some(format!("a {temp:.0}°C morning, so the water is cold")),
            _ => None,
        })
        .flatten();
    let needs_for = |charge: EvCharge, ev_wh: f64, ev_required: bool, geyser_required: bool| -> Vec<Need> {
        let ev = |priority: EvCharge| Need {
            kind: LoadKind::Ev,
            levels: vec![config.ev_charger_watts],
            need_wh: if charge == priority { ev_wh } else { 0.0 },
            from: now,
            until: sunset,
            running_w: ev_running.as_ref().map(|r| r.watts),
            partial: true,
            session_h: None,
            required: ev_required && priority == EvCharge::Yes,
        };
        let geyser = Need {
            kind: LoadKind::Geyser,
            levels: geyser_levels(config),
            need_wh: geyser_need,
            from: at_hour(today, config.geyser_start_hour),
            until: geyser_until,
            running_w: geyser_running.as_ref().map(|r| r.watts),
            partial: true,
            session_h: None,
            required: geyser_required,
        };
        let mut needs = vec![
            Need {
                kind: LoadKind::Water,
                levels: vec![config.water_motor_watts],
                need_wh: water_need,
                from: at_hour(today, config.water_start_hour),
                until: at_hour(today, config.water_end_hour).min(sunset),
                running_w: water_running.as_ref().map(|r| r.watts),
                partial: water_running.is_some(),
                session_h: None,
                required: false,
            },
            ev(EvCharge::Yes),
        ];
        match hotplate_need.clone() {
            Some(hotplate) if geyser_first.is_some() => needs.extend([geyser, hotplate]),
            Some(hotplate) => needs.extend([hotplate, geyser]),
            None => needs.push(geyser),
        }
        needs.push(ev(EvCharge::IfSpare));
        needs
    };

    let typical = input.typical_load;
    let pv_at = |at: NaiveDateTime| forecast_pv(at);
    let sun = Sun {
        now,
        sunset,
        soc: input.soc.unwrap_or(0.0),
        rated_ah: input.rated_ah,
        volts: input.volts,
        max_charge_a: input.max_charge_a,
        floor: config.min_soc_percent,
        day_low: config.day_low_soc,
        pv_now_w,
        house_now_w,
        pv_at: &pv_at,
        house_at: typical,
    };
    let daytime = input.daytime && soc_ready && now < sunset;

    let cooking_anchor = if cooking_day {
        Some(today)
    } else if hotplate_skipped {
        Some(today - Duration::days(i64::from(config.hotplate_every_days) - 1))
    } else {
        input.hotplate_on
    };
    let upcoming = {
        let house_kwh = (7..17).filter_map(typical).sum::<f64>() / 1000.0;
        let battery_kwh = input.tonight_needs_soc.unwrap_or(50.0) / 100.0 * input.rated_ah * input.volts / 1000.0;
        next_days(input, house_kwh, battery_kwh, cooking_anchor.filter(|_| config.hotplate_enabled))
    };
    let ev_first_wh = if daytime && ev_need_full > 0.0 {
        plan(&sun, &needs_for(EvCharge::Yes, ev_need_full, false, false)).map_or(0.0, |first| slot_wh(&first.slots, LoadKind::Ev))
    } else {
        0.0
    };
    let ev_rule = ev_soc
        .filter(|_| config.ev_enabled)
        .map(|soc| ev_today(soc, config, upcoming.first(), ev_first_wh >= EV_SUNNY_SHARE * ev_need_full, tomorrow_geyser_kwh));
    let ev_rule_skips = matches!(ev_rule, Some(EvToday::NotNeeded(_))) && ev_running.is_none();
    let ev_need = if ev_rule_skips { 0.0 } else { ev_need_full };
    let probe = daytime.then(|| plan(&sun, &needs_for(EvCharge::IfSpare, ev_need, false, false))).flatten();
    let ev_charge = if matches!(ev_rule, Some(EvToday::Needed(_))) {
        EvCharge::Yes
    } else {
        decision.as_ref().map(|d| d.charge).unwrap_or_else(|| {
        ev_soc.map_or(EvCharge::IfSpare, |soc| {
            fallback_ev_charge(soc, config, probe.as_ref().map(|p| p.spare_before_ev_wh), upcoming.first().and_then(|d| d.expected_spare_kwh), tomorrow_geyser_kwh)
        })
        })
    };
    let ev_required = matches!(ev_rule, Some(EvToday::Needed(_)));
    let ai_live = input.ai_available && memory.ai.live(now, &ai_state);
    let planned = if ai_live {
        daytime.then(|| ai::plan_from_slots(&sun, &memory.ai.slots)).flatten()
    } else {
        daytime.then(|| plan(&sun, &needs_for(ev_charge, ev_need, ev_required, geyser_required.is_some()))).flatten()
    };
    let ai_note = |kind: LoadKind, verdict: Verdict| -> Verdict {
        match memory.ai.note(kind).filter(|_| ai_live && !matches!(verdict.phase, LoadPhase::Off | LoadPhase::Ask | LoadPhase::Stop)) {
            Some(note) => Verdict { note: note.to_string(), ..verdict },
            None => verdict,
        }
    };
    let sun_fills_at = planned
        .as_ref()
        .and_then(|p| p.full_at)
        .filter(|_| daytime && !held_back && input.battery_w.is_some_and(|watts| watts >= 0.0));
    let slot_for = |kind: LoadKind| planned.as_ref().and_then(|p| p.slots.iter().find(|s| s.kind == kind).cloned());

    let mut notices = Notices {
        now,
        repeat: Duration::minutes(i64::from(config.water_repeat_minutes)),
        rows: &mut memory.rows,
        out: Vec::new(),
    };

    let remote = config.geyser_auto_control && input.remote_ready;
    let auto = remote && memory.geyser.failed_on != Some(today);
    let watching = watch_geyser(&mut memory.geyser, geyser_running.as_ref(), input, sunset);
    let reheat_from = geyser_until - Duration::hours(GEYSER_REHEAT_HOURS_BEFORE_END);
    let reheat_open = auto
        && geyser_season
        && daytime
        && !geyser_done
        && memory.geyser.reheated_at.is_none()
        && memory.geyser_heated_at.is_some_and(|at| now - at >= Duration::minutes(GEYSER_REHEAT_AFTER_HOT_MINUTES))
        && now >= reheat_from
        && now < geyser_until - Duration::minutes(GEYSER_REHEAT_LAST_MINUTES);
    let reheating = memory.geyser.reheat_since.is_some() && geyser_running.is_some();
    let night_repeat = Duration::hours(12);
    let day_repeat = Duration::zero();
    let geyser_verdict = if !config.geyser_enabled {
        Verdict { phase: LoadPhase::Off, note: "Geyser planning is off.".into(), watts: None, slot: None }
    } else if !geyser_season {
        Verdict { phase: LoadPhase::Off, note: OFF_SEASON_NOTE.into(), watts: None, slot: None }
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
        let note = match (memory.geyser_heated_at, memory.geyser.reheated_at) {
            (Some(at), Some(again)) => format!(
                "Hot: the thermostat cut out at {} after {:.1} kWh and again at {} after the top-up for the night.",
                clock_time(at),
                memory.geyser_delivered_wh / 1000.0,
                clock_time(again)
            ),
            (Some(at), None) => {
                let later = if auto && !geyser_done && now < geyser_until - Duration::minutes(GEYSER_REHEAT_LAST_MINUTES) {
                    format!(" From {} it tops up once more on spare sun so it lasts till morning.", clock_time(reheat_from))
                } else {
                    String::new()
                };
                format!(
                    "Went off at {} after {:.1} kWh, taken as the thermostat: the water is hot. Switched it off yourself? Tap Change.{later}",
                    clock_time(at),
                    memory.geyser_delivered_wh / 1000.0
                )
            }
            _ => "Marked done for today.".into(),
        };
        Verdict { phase: LoadPhase::Done, note, watts: None, slot: None }
    } else {
        let slot = slot_for(LoadKind::Geyser);
        let (phase, watts) = judge(slot.as_ref(), geyser_running.as_ref(), now);
        let note = match (phase, &slot) {
            (LoadPhase::Ready, Some(slot)) if slot.topped_up => format!(
                "Turn the geyser on at {} until about {}. Not enough spare sun today, so the grid tops it up.",
                level_text(slot.watts, config),
                clock_time(slot.end)
            ),
            (LoadPhase::Running, Some(slot)) if slot.topped_up => format!(
                "Heating at {} until about {}; the grid tops up what the sun can't.",
                level_text(slot.watts, config),
                clock_time(slot.end)
            ),
            (LoadPhase::Waiting, Some(slot)) if slot.topped_up => format!(
                "Best time for it: {} to {} at {}; the grid tops up what the sun can't.",
                clock_time(slot.start),
                clock_time(slot.end),
                level_text(slot.watts, config)
            ),
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
    let geyser_verdict = ai_note(LoadKind::Geyser, geyser_verdict);
    let unused_sun = steps(&sun).first().map(|step| {
        let others: f64 = [&water_running, &ev_running].iter().filter_map(|r| r.as_ref().map(|r| r.watts)).sum();
        let hotplate = hotplate_w.filter(|_| memory.hotplate.running.is_some()).unwrap_or(0.0);
        let usual = typical(now.hour()).unwrap_or(step.house_w);
        let known = usual + others + geyser_running.as_ref().map_or(0.0, |r| r.watts);
        (step.pv_w - step.house_w - others - hotplate, input.load_w.map_or(0.0, |load| (load - known).max(0.0)))
    });
    let raw_unknown = unused_sun.map_or(0.0, |(_, unknown)| unknown);
    memory.geyser.not_hotplate = memory
        .geyser
        .not_hotplate
        .filter(|(at, _)| raw_unknown >= HOTPLATE_OFF_W && now - *at < Duration::minutes(GEYSER_NOT_HOTPLATE_MINUTES));
    let cooking_w = hotplate_w.filter(|_| esp32 && memory.hotplate.running.is_some()).unwrap_or(0.0);
    let unknown_w = (raw_unknown - cooking_w - memory.geyser.not_hotplate.map_or(0.0, |(_, watts)| watts)).max(0.0);
    memory.geyser.unknown_w = unknown_w;
    let in_window = now >= at_hour(today, config.geyser_start_hour) && now < geyser_until;
    let sun_now = unused_sun.filter(|_| auto && in_window).map(|(spare, _)| SunNow {
        spare,
        full: held_back,
        explained: unknown_w <= GEYSER_UNKNOWN_LOAD_W,
        draining: input.battery_w.is_some_and(|watts| watts < -GEYSER_SUN_DRAIN_W),
        step_up_ok: memory.geyser.pending.is_none()
            && memory.geyser.last_sent_at.is_none_or(|at| now - at >= Duration::minutes(GEYSER_STEP_UP_MINUTES))
            && memory.geyser.stepped_down_at.is_none_or(|at| now - at >= Duration::minutes(GEYSER_UP_AFTER_DOWN_MINUTES))
            && memory.geyser.readings_seen >= READINGS_BEFORE_STEP_UP,
    });
    let hot = geyser_done || memory.geyser_heated_at.is_some();
    let sun_wasted = sun_now.as_ref().filter(|sun| {
        sun.full && !sun.explained && geyser_levels(config).into_iter().any(|level| level <= sun.spare + GEYSER_LEND_W)
    });
    if let (Some(sun), Some(load)) = (sun_wasted, input.load_w) {
        let free_to_ask = (!hot || reheat_open)
            && geyser_running.is_none()
            && water_running.is_none()
            && memory.hotplate.running.is_none()
            && memory.hotplate.asking.is_none()
            && memory.ai.question.is_none()
            && memory.geyser.asked_unknown_at.is_none_or(|at| now - at >= Duration::minutes(GEYSER_ASK_GAP_MINUTES));
        if free_to_ask {
            tracing::info!("loads: {raw_unknown:.0} W the app can't place keeps the geyser off at {} — asking", clock_time(now));
            memory.geyser.asked_unknown_at = Some(now);
            memory.hotplate.asking = Some(HotplateAsk { at: now, base_w: (load - flexible_w - raw_unknown).max(0.0), step_w: raw_unknown, change: None });
            result.asks.push(Ask {
                title: HOTPLATE_TITLE,
                body: format!(
                    "The battery is full and about {:.1} kW of sun is going spare, but about {raw_unknown:.0} W is on that the app can't place. Is the hotplate on? Say No and the geyser takes the sun.",
                    sun.spare / 1000.0
                ),
                about: AskAbout::Loads,
            });
        }
    }
    let hotplate_maybe_on = memory.hotplate.running.is_some() || water_running.is_some() || memory.geyser.unknown_w >= HOTPLATE_SEEN_W;
    let geyser_verdict = match (sun_now, memory.geyser_heated_at) {
        (Some(sun_now), _) if !hot || reheating => sun_for_geyser(geyser_verdict, &sun_now, geyser_running.as_ref(), config),
        (Some(sun_now), Some(heated_at)) if reheat_open && geyser_running.is_none() => {
            let stepped_down = memory.geyser.stepped_down_at.is_some_and(|at| now - at < Duration::minutes(GEYSER_UP_AFTER_DOWN_MINUTES));
            reheat_verdict(&sun_now, heated_at, hotplate_maybe_on || stepped_down, config)
        }
        _ => geyser_verdict,
    };
    let after_sunset = !input.daytime || now >= sunset;
    let draining_w = input
        .battery_w
        .map(|watts| -watts)
        .filter(|watts| *watts > GEYSER_DRAIN_ACT_W && geyser_running.is_some() && auto && !after_sunset);
    let drain_started = match draining_w {
        Some(drain) => {
            memory.geyser.drain_w = drain;
            if let Some(watch) = memory.geyser.watch.as_mut() {
                watch.ended_at.get_or_insert(now);
                watch.announced = true;
            }
            memory.geyser.drain_since.replace(memory.geyser.drain_since.unwrap_or(now)).is_none()
        }
        None => {
            memory.geyser.drain_since = None;
            false
        }
    };
    let watching = watching.filter(|_| memory.geyser.drain_since.is_none());
    let geyser_verdict = match (&geyser_running, watching) {
        (Some(running), Some(until)) if geyser_verdict.phase != LoadPhase::Off => Verdict {
            phase: LoadPhase::Running,
            note: format!(
                "Just on at {}: watching the sun and battery until {} before changing anything.",
                level_text(running.watts, config),
                clock_time(until)
            ),
            watts: None,
            slot: geyser_verdict.slot,
        },
        (Some(_), _) if memory.geyser.night_hold && after_sunset => Verdict {
            phase: LoadPhase::Running,
            note: "On by hand for the night: it stops by itself when the water is hot, or switch it off.".into(),
            watts: None,
            slot: None,
        },
        _ => geyser_verdict,
    };
    let geyser_verdict = match (&geyser_running, memory.geyser.drain_since) {
        (Some(running), Some(since))
            if now - since >= Duration::minutes(GEYSER_DRAIN_GRACE_MINUTES) && geyser_verdict.phase != LoadPhase::Stop =>
        {
            turn_down_for_drain(running, memory.geyser.drain_w, config)
        }
        _ => geyser_verdict,
    };
    let geyser_room = geyser_room(input, &geyser_running, &water_running, &ev_running, hotplate_maybe_on, config);
    memory.geyser.room_w = geyser_room;
    memory.geyser.urgent = auto && geyser_running.as_ref().is_some_and(|running| running.watts > geyser_room + 1.0);
    let geyser_verdict = match &geyser_running {
        Some(running) if memory.geyser.urgent => keep_inverter_safe(running, geyser_room, hotplate_maybe_on, config),
        Some(running) if geyser_verdict.phase == LoadPhase::Running && geyser_verdict.watts.is_some_and(|watts| watts > geyser_room + 1.0) => {
            Verdict { watts: Some(running.watts), ..geyser_verdict }
        }
        None if geyser_verdict.phase == LoadPhase::Ready && geyser_verdict.watts.is_some_and(|watts| watts > geyser_room + 1.0) => {
            match geyser_levels(config).into_iter().find(|level| *level <= geyser_room) {
                Some(level) if hotplate_maybe_on => Verdict {
                    note: format!(
                        "A big load is on (the motor or the hotplate): geyser on at {} only. Keep the hotplate at {HOTPLATE_KEEP_BELOW_W:.0} W or below.",
                        level_text(level, config)
                    ),
                    watts: Some(level),
                    ..geyser_verdict
                },
                Some(level) => Verdict { watts: Some(level), ..geyser_verdict },
                _ => Verdict {
                    phase: LoadPhase::Waiting,
                    note: if hotplate_maybe_on {
                        format!(
                            "The house is using a lot with a big load on: the geyser waits until it's below {HOTPLATE_GEYSER_ON_W:.0} W, then comes on at {}.",
                            level_text(config.geyser_low_watts, config)
                        )
                    } else {
                        "The house is already near the inverter's limit: the geyser waits.".into()
                    },
                    watts: None,
                    slot: geyser_verdict.slot,
                },
            }
        }
        _ => geyser_verdict,
    };
    if let Some(message) = watch_decision(&mut memory.geyser, memory.ai.decided_at, &geyser_verdict, input.ai_available, auto, now, config) {
        infos.push((GEYSER_TITLE, message));
    }
    if geyser_verdict.phase == LoadPhase::Skipped && soc_ready && !memory.geyser_weak_notified && now < geyser_until && input.forecast.is_some() {
        memory.geyser_weak_notified = true;
        infos.push((GEYSER_TITLE, format!("{} Use gas or heat it later from the grid at 800 W if you need it.", geyser_verdict.note)));
    }
    let repeat = if daytime { day_repeat } else { night_repeat };
    let geyser_verdict = sun_carries(geyser_verdict, geyser_running.as_ref().map(|r| r.watts), carried);
    let geyser_verdict = match &geyser_running {
        _ if geyser_season || !config.geyser_enabled => geyser_verdict,
        Some(r) => Verdict { phase: LoadPhase::Running, note: format!("On by hand at about {:.0} W; outside winter the app leaves it to you.", r.watts), watts: Some(r.watts), slot: None },
        None => Verdict { phase: LoadPhase::Off, note: OFF_SEASON_NOTE.into(), watts: None, slot: None },
    };
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
    } else if let (Some(EvToday::NotNeeded(why)), true) = (&ev_rule, ev_rule_skips) {
        Verdict { phase: LoadPhase::Done, note: format!("No charge needed: {why}."), watts: None, slot: None }
    } else {
        let slot = slot_for(LoadKind::Ev);
        let (phase, watts) = judge(slot.as_ref(), ev_running.as_ref(), now);
        let hours = ev_need_wh(ev_soc.unwrap_or(100.0), config) / config.ev_charger_watts;
        let note = match (phase, &slot) {
            (LoadPhase::Ready, Some(slot)) if slot.topped_up => format!(
                "Plug in the scooter ({:.0}%, about {} to full) until about {}. Not enough spare sun today, so the grid tops it up.",
                ev_soc.unwrap_or_default(),
                duration_text(hours),
                clock_time(slot.end)
            ),
            (LoadPhase::Running, Some(slot)) if slot.topped_up => {
                format!("Charging until about {}; the grid tops up what the sun can't.", clock_time(slot.end))
            }
            (LoadPhase::Waiting, Some(slot)) if slot.topped_up => format!(
                "Best time to charge: {} to {}; the grid tops up what the sun can't.",
                clock_time(slot.start),
                clock_time(slot.end)
            ),
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
    let ev_verdict = if ev_rule_skips { ev_verdict } else { ai_note(LoadKind::Ev, ev_verdict) };
    let ev_repeat = if daytime { Duration::minutes(i64::from(config.ev_remind_minutes)) } else { night_repeat };
    let ev_quiet = matches!(ev_rule, Some(EvToday::NotNeeded(_)));
    let ev_verdict = sun_carries(ev_verdict, ev_running.as_ref().map(|r| r.watts), carried);
    let ev_verdict = sun_covers(ev_verdict, ev_running.as_ref().map(|r| r.watts), sun_fills_at, input.daytime && !soc_ready);
    notices.track(LoadKind::Ev, EV_TITLE, ev_verdict.phase, &ev_verdict.note, ev_quiet, ev_repeat);

    let hotplate_slot = slot_for(LoadKind::Hotplate);
    let hotplate_running = match (&memory.hotplate.running, hotplate_w) {
        (Some(run), Some(watts)) => Some(Running { watts, since: run.since }),
        _ => None,
    };
    let mut hotplate_weak = false;
    let hotplate_verdict = if !config.hotplate_enabled {
        Verdict { phase: LoadPhase::Off, note: "Hotplate planning is off.".into(), watts: None, slot: None }
    } else if let Some(ask) = &memory.hotplate.asking {
        Verdict {
            phase: LoadPhase::Ask,
            note: format!("A {:.1} kW load came on at {} — is it the hotplate?", ask.step_w / 1000.0, clock_time(ask.at)),
            watts: None,
            slot: None,
        }
    } else if let Some(running) = &hotplate_running {
        if daytime {
            let (phase, watts) = judge(hotplate_slot.as_ref(), Some(running), now);
            let note = match (phase, &hotplate_slot) {
                (LoadPhase::Running, Some(slot)) => {
                    format!("Cooking at about {:.0} W. The sun can carry it until about {}.", running.watts, clock_time(slot.end))
                }
                (LoadPhase::Stop, Some(slot)) if watts.is_some() => {
                    format!("Turn the hotplate down to about {:.0} W — the sun can't carry {:.0} W.", slot.watts, running.watts)
                }
                _ => "The sun can't carry the hotplate now — turn it right down or finish on gas.".to_string(),
            };
            Verdict { phase, note, watts, slot: hotplate_slot.clone() }
        } else {
            Verdict { phase: LoadPhase::Running, note: "Cooking after sunset — the night plan covers it.".into(), watts: None, slot: None }
        }
    } else if memory.hotplate.running.is_some() {
        Verdict { phase: LoadPhase::Running, note: "Cooking.".into(), watts: None, slot: None }
    } else if hotplate_skipped {
        Verdict { phase: LoadPhase::Skipped, note: "Not cooking on the hotplate today.".into(), watts: None, slot: None }
    } else if cooked_today {
        Verdict { phase: LoadPhase::Done, note: "Cooked today.".into(), watts: None, slot: None }
    } else if !cooking_day {
        let next = input.hotplate_on.map(|last| last + Duration::days(i64::from(config.hotplate_every_days))).filter(|date| *date > today);
        let note = next.map_or("Not a cooking day.".into(), |date| format!("Not a cooking day — next one {}.", date.format("%A")));
        Verdict { phase: LoadPhase::Skipped, note, watts: None, slot: None }
    } else if now >= hotplate_until {
        Verdict { phase: LoadPhase::Skipped, note: "Today's cooking window is over.".into(), watts: None, slot: None }
    } else if !daytime {
        Verdict { phase: LoadPhase::Waiting, note: "Cooking day — planned in the sun.".into(), watts: None, slot: None }
    } else {
        let (phase, watts) = judge(hotplate_slot.as_ref(), None, now);
        let session = Duration::minutes((config.hotplate_hours * 60.0).round() as i64);
        let note = match (phase, &hotplate_slot) {
            (LoadPhase::Ready, Some(slot)) => {
                format!("Good sun to cook on the hotplate now — up to about {:.0} W until {}.", slot.watts, clock_time(slot.end))
            }
            (LoadPhase::Waiting, Some(slot)) => format!(
                "Best sun for cooking: {} to {}, up to about {:.0} W.",
                clock_time(slot.start),
                clock_time(slot.end),
                slot.watts
            ),
            _ if now + session > hotplate_until => "Not enough of the cooking window left for a full session on the sun.".to_string(),
            _ => {
                hotplate_weak = true;
                "Not enough sun for the hotplate today — cook on gas.".to_string()
            }
        };
        Verdict { phase, note, watts, slot: hotplate_slot.clone() }
    };
    if hotplate_weak && !memory.hotplate.weak_notified && input.forecast.is_some() {
        memory.hotplate.weak_notified = true;
        notices.out.push((HOTPLATE_TITLE, hotplate_verdict.note.clone()));
    }
    let hotplate_verdict = ai_note(LoadKind::Hotplate, hotplate_verdict);
    let quiet = hotplate_verdict.phase != LoadPhase::Stop;
    let hotplate_verdict = sun_carries(hotplate_verdict, hotplate_w, carried);
    notices.track(LoadKind::Hotplate, HOTPLATE_TITLE, hotplate_verdict.phase, &hotplate_verdict.note, quiet, repeat);

    let water_slot = slot_for(LoadKind::Water);
    let water_verdict = if !config.water_reminder_enabled {
        Verdict { phase: LoadPhase::Off, note: "Reminders are off.".into(), watts: None, slot: None }
    } else if !input.water_pending && water_running.is_none() {
        Verdict { phase: LoadPhase::Done, note: "Tank filled today.".into(), watts: None, slot: None }
    } else if !daytime {
        Verdict { phase: LoadPhase::Waiting, note: "Waiting for good solar.".into(), watts: None, slot: None }
    } else {
        let (phase, watts) = judge(water_slot.as_ref(), water_running.as_ref(), now);
        let note = match (phase, &water_slot) {
            (LoadPhase::Ready, Some(slot)) => format!("Good sun: turn on the motor. The sun can carry it until about {}.", clock_time(slot.end)),
            (LoadPhase::Running, Some(slot)) => format!("Motor running. Fine until about {}.", clock_time(slot.end)),
            (LoadPhase::Stop, _) => "Switch the motor off — the sun can't carry it now.".to_string(),
            (LoadPhase::Waiting, Some(slot)) => format!("Best sun for the tank: {} to {}.", clock_time(slot.start), clock_time(slot.end)),
            _ => "Not enough sun for the tank today.".to_string(),
        };
        Verdict { phase, note, watts, slot: water_slot.clone() }
    };
    let water_verdict = sun_carries(ai_note(LoadKind::Water, water_verdict), water_running.as_ref().map(|r| r.watts), carried);
    let water_verdict = sun_covers(water_verdict, water_running.as_ref().map(|r| r.watts), sun_fills_at, input.daytime && !soc_ready);
    if ai_live {
        notices.track(LoadKind::Water, crate::automation::water::TITLE, water_verdict.phase, &water_verdict.note, false, repeat);
    }
    if let Some((question, _)) = memory.ai.question.as_ref().filter(|_| !memory.ai.question_notified) {
        result.asks.push(Ask { title: LOADS_TITLE, body: question.text.clone(), about: AskAbout::Loads });
        memory.ai.question_notified = true;
    }
    let mut out = std::mem::take(&mut notices.out);
    out.extend(infos);
    result.notify = out;

    let control = if auto {
        Some(Control::Full)
    } else if remote {
        Some(Control::OffOnly)
    } else {
        memory.geyser.pending.map(|_| Control::VerifyOnly)
    };
    if input.load_w != memory.geyser.last_load_w {
        memory.geyser.last_load_w = input.load_w;
        memory.geyser.reading_at = Some(now);
        memory.geyser.readings_seen += 1;
    }
    let settled_after_start = memory.geyser.readings_seen >= READINGS_BEFORE_PRESSING;
    if !settled_after_start {
        if input.geyser_request.is_some() {
            result.notify.push((GEYSER_TITLE, "The app has just started and is waiting for a fresh inverter reading; nothing pressed. Try again in a few minutes.".into()));
        }
    } else if let Some(request) = input.geyser_request {
        result.geyser_command = manual_geyser(memory, request, geyser_running.as_ref(), !after_sunset, config, &mut result.notify);
    } else if let Some(control) = control.filter(|_| geyser_season) {
        result.geyser_command = geyser_command(memory, &geyser_verdict, geyser_running.as_ref(), control, now, config, &mut result.notify);
        match result.geyser_command.as_ref().map(|command| command.intent) {
            Some(GeyserIntent::On(_)) if reheat_open && geyser_running.is_none() => memory.geyser.reheat_since = Some(now),
            Some(GeyserIntent::Off) => memory.geyser.reheat_since = None,
            _ => {}
        }
        if hotplate_maybe_on && geyser_running.is_none() && result.geyser_command.as_ref().is_some_and(|command| matches!(command.intent, GeyserIntent::On(_))) {
            result.notify.push((
                GEYSER_TITLE,
                format!(
                    "Geyser on at {} beside the motor or the hotplate. Keep the hotplate at {HOTPLATE_KEEP_BELOW_W:.0} W or below; above {HOTPLATE_GEYSER_OFF_W:.0} W in the house the geyser goes off.",
                    level_text(config.geyser_low_watts, config)
                ),
            ));
        }
    }

    let check_at = today.and_time(chrono::NaiveTime::MIN) + Duration::minutes(i64::from(config.ev_check_minute));
    let checked_at = memory.ev.last_live.map(|(_, at)| at).filter(|at| *at >= check_at);
    let checked = input.ev_checked_on == Some(today) || checked_at.is_some();
    if memory.ev.check.day != Some(today) {
        memory.ev.check = EvCheck { day: Some(today), ..EvCheck::default() };
    }
    if config.ev_enabled && !input.ev_reader_busy {
        let since = |at: Option<NaiveDateTime>, minutes: i64| at.is_none_or(|at| now - at >= Duration::minutes(minutes));
        let live_due = charging && config.ev_power_cycle_reads && since(memory.ev.last_live_request, EV_LIVE_READ_MINUTES);
        let check_due = config.ev_daily_check
            && input.daytime
            && now >= check_at
            && !checked
            && memory.ev.check.attempts < EV_CHECK_ATTEMPTS
            && since(memory.ev.check.last_at, EV_CHECK_RETRY_MINUTES);
        if check_due {
            memory.ev.check.attempts += 1;
            memory.ev.check.last_at = Some(now);
        }
        let passive_due = since(memory.ev.last_read_request, if input.daytime { EV_READ_MINUTES } else { EV_NIGHT_READ_MINUTES });
        if live_due || check_due {
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

    if drain_started || memory.geyser.watch.as_ref().is_some_and(|watch| watch.ended_at == Some(now) && !watch.announced) {
        memory.ai.next_at = Some(now);
    }
    if input.ai_available && daytime && memory.ai.due(now, input.load_w, &ai_state) {
        let app_plan = plan(&sun, &needs_for(ev_charge, ev_need, ev_required, geyser_required.is_some()));
        let required_kinds: Vec<LoadKind> = [(LoadKind::Geyser, geyser_required.is_some()), (LoadKind::Ev, ev_required)]
            .into_iter()
            .filter_map(|(kind, required)| required.then_some(kind))
            .collect();
        let facts = ai::Facts {
            input,
            sun: &sun,
            memory,
            house_usual_w: typical(now.hour()).or(house_now_w).unwrap_or(0.0),
            pv_held_back: held_back,
            water_running: water_running.as_ref(),
            geyser_running: geyser_running.as_ref(),
            ev_running: ev_running.as_ref(),
            hotplate_running: hotplate_running.clone(),
            geyser_done,
            ev_skipped,
            hotplate_skipped,
            cooking_day,
            cooked_today,
            geyser_first: geyser_first.as_deref(),
            next_days: upcoming.clone(),
            ev_rule: ev_rule.as_ref(),
            geyser_required: geyser_required.as_deref(),
            app_grid_wh: app_plan.as_ref().filter(|_| !required_kinds.is_empty()).map(|p| ai::required_grid_wh(&sun, &p.slots, &required_kinds)),
            geyser_off: !geyser_free && geyser_running.is_none(),
        };
        result.ai_request = Some((ai::ai_input(&facts), ai::checker(&facts)));
        memory.ai_asked(now, input.load_w, ai_state.clone());
    }
    result.hotplate_cooked = daytime && memory.hotplate.running.as_ref().is_some_and(|run| run.counts(now)) && !cooked_today;
    result.ev_checked = checked_at.is_some() && input.ev_checked_on != Some(today);
    let check_failed = config.ev_daily_check
        && !checked
        && !input.ev_reader_busy
        && memory.ev.check.attempts >= EV_CHECK_ATTEMPTS
        && memory.ev.check.last_at.is_some_and(|at| now - at >= Duration::minutes(2));
    let running_now: Vec<(LoadKind, f64)> = [
        (LoadKind::Water, water_running.as_ref().map(|r| r.watts)),
        (LoadKind::Geyser, geyser_running.as_ref().map(|r| r.watts)),
        (LoadKind::Ev, ev_running.as_ref().map(|r| r.watts)),
        (LoadKind::Hotplate, hotplate_w.filter(|_| hotplate_active)),
    ]
    .into_iter()
    .filter_map(|(kind, watts)| watts.map(|watts| (kind, watts)))
    .collect();
    let sun_hours = if daytime {
        sun_hours(&steps(&sun), &running_now, planned.as_ref().map_or(&[][..], |p| p.slots.as_slice()))
    } else {
        Vec::new()
    };

    let active = [&geyser_verdict.phase, &ev_verdict.phase, &hotplate_verdict.phase]
        .iter()
        .any(|phase| matches!(phase, LoadPhase::Ready | LoadPhase::Running | LoadPhase::Stop | LoadPhase::Ask))
        || water_running.is_some()
        || memory.geyser.pending.is_some();
    let tick = if memory.geyser.drain_since.is_some() {
        1
    } else if active {
        ACTIVE_TICK_MINUTES
    } else {
        IDLE_TICK_MINUTES
    };
    result.next_at = Some(now + Duration::minutes(tick));

    memory.grid_now = planned.as_ref().filter(|p| daytime && p.grid_now).map(|p| {
        let names: Vec<String> = p
            .slots
            .iter()
            .filter(|slot| slot.start <= now + Duration::minutes(1) && slot.end > now)
            .map(|slot| format!("the {}", ai::load_name(slot.kind)))
            .collect();
        if names.is_empty() { "the loads on now".to_string() } else { names.join(" and ") }
    });
    let own_press = |at: NaiveDateTime| memory.geyser.last_sent_at.is_some_and(|sent| (at - sent).num_seconds().abs() <= 30);
    let new_press = |at: NaiveDateTime| memory.elements.at.is_none_or(|seen| at > seen + Duration::seconds(HEARD_SAME_PRESS_SECONDS));
    if let Some(heard) = input.geyser_heard_at.filter(|at| !own_press(*at) && new_press(*at)) {
        let flip = match input.geyser_heard_button {
            Some(Button::W800) => Some((true, false)),
            Some(Button::W1200) => Some((false, true)),
            _ => None,
        };
        match (memory.elements.selected, flip) {
            (Some((low, high)), Some((flip_low, flip_high))) => {
                let button = input.geyser_heard_button.map_or("", |button| button.label());
                let unsure = memory.elements.unsure;
                memory.elements.set((low ^ flip_low, high ^ flip_high), heard, format!("{button} pressed on the remote at {}", clock_time(heard)));
                memory.elements.unsure = unsure;
            }
            _ => {
                memory.elements.unsure = true;
                memory.elements.at = Some(heard);
                memory.elements.why = format!("a button was pressed on the remote at {}", clock_time(heard));
            }
        }
    }
    if let Some((running, selected)) = geyser_running.as_ref().and_then(|r| geyser_elements(r.watts, config).map(|selected| (r, selected))) {
        if memory.elements.selected != Some(selected) || memory.elements.unsure {
            memory.elements.set(selected, now, format!("the load shows it drawing about {:.0} W", running.watts));
        }
    }
    let soc = memory.ev.soc.as_ref();
    result.status = LoadsStatus {
        geyser: GeyserStatus {
            row: row_from(&geyser_verdict, geyser_running.as_ref(), memory.rows.get(&LoadKind::Geyser)),
            delivered_kwh: (memory.geyser_delivered_wh / 100.0).round() / 10.0,
            need_kwh: config.geyser_daily_kwh,
            heated_at: memory.geyser_heated_at,
            auto,
            check: memory.geyser.check.clone(),
            watch_until: watching,
            elements: geyser_elements_status(&memory.elements, geyser_running.is_some()),
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
            charge_check: memory.ev.charge_check.clone(),
            charge_today: ev_rule.as_ref().map(|rule| matches!(rule, EvToday::Needed(_))),
            charge_rule: ev_rule.as_ref().map(|rule| match rule {
                EvToday::Needed(why) => format!("Charge today: {why}."),
                EvToday::NotNeeded(why) => format!("No charge needed: {why}."),
            }),
            checked_at,
            check_failed,
        },
        hotplate: HotplateStatus {
            row: row_from(&hotplate_verdict, hotplate_running.as_ref(), memory.rows.get(&LoadKind::Hotplate)),
            cooking_day,
            asking_w: memory.hotplate.asking.as_ref().map(|a| a.step_w),
            asking_since: memory.hotplate.asking.as_ref().map(|a| a.at),
        },
        water: row_from(&water_verdict, water_running.as_ref(), memory.rows.get(&LoadKind::Water)),
        geyser_first: geyser_first.filter(|_| cooking_day && geyser_season),
        question: memory.ai.question.as_ref().map(|(question, _)| question.clone()).or_else(|| {
            memory.hotplate.asking.as_ref().map(|ask| LoadsQuestion {
                text: format!("A {:.1} kW load came on at {} — is it the hotplate?", ask.step_w / 1000.0, clock_time(ask.at)),
                at: ask.at,
            })
        }),
        ai: AiStatus {
            mode: if ai_live {
                AiMode::Ai
            } else if input.ai_available && memory.ai.mode == AiMode::Fallback {
                AiMode::Fallback
            } else {
                AiMode::Off
            },
            ..memory.ai.status()
        },
        water_running_since: water_running.map(|r| r.since),
        spare_kwh: planned.as_ref().map(|p| (p.spare_wh / 100.0).round() / 10.0),
        sunset_target_soc: planned.as_ref().map(|p| p.sun_only_soc.round()),
        projected_sunset_soc: planned.as_ref().map(|p| p.end_soc.round()),
        full_by: planned.as_ref().and_then(|p| p.full_at),
        pv_ratio: Some((input.pv_ratio * 100.0).round() / 100.0),
        grid_now: memory.grid_now.is_some(),
        grid_kwh: planned.as_ref().filter(|p| p.grid_wh > 1.0).map(|p| (p.grid_wh / 100.0).round() / 10.0),
        geyser_required: geyser_required.clone(),
        plan: planned.map(|p| p.slots).unwrap_or_default(),
        sun_hours,
        tank_question: memory.water_ask,
    };
    let fresh = memory.geyser.reading_at == Some(now);
    let acted = result.geyser_command.is_some()
        || !result.asks.is_empty()
        || !result.notify.is_empty()
        || input.answer.is_some()
        || !input.edits.is_empty()
        || input.geyser_request.is_some()
        || !changes.is_empty();
    if fresh || acted {
        let row = |row: &LoadRow| serde_json::json!({ "phase": row.phase, "watts": row.watts, "note": row.note });
        result.log = Some(serde_json::json!({
            "at": now,
            "fresh": fresh,
            "load_w": input.load_w,
            "pv_w": input.pv_w,
            "soc": input.soc,
            "battery_w": input.battery_w,
            "usual_w": typical(now.hour()),
            "full": held_back,
            "spare_w": unused_sun.map(|(spare, _)| spare.round()),
            "unknown_w": memory.geyser.unknown_w,
            "not_hotplate_w": memory.geyser.not_hotplate.map(|(_, watts)| watts),
            "room_w": memory.geyser.room_w,
            "hotplate_maybe_on": hotplate_maybe_on,
            "running": memory.detector.running.iter().map(|(kind, r)| (format!("{kind:?}"), r.watts)).collect::<BTreeMap<_, _>>(),
            "hotplate": { "running": memory.hotplate.running.is_some(), "asking_w": memory.hotplate.asking.as_ref().map(|a| a.step_w), "share_w": hotplate_w },
            "geyser_delivered_kwh": (memory.geyser_delivered_wh / 10.0).round() / 100.0,
            "geyser_check": result.status.geyser.check,
            "reheat": { "open": reheat_open, "since": memory.geyser.reheat_since, "done_at": memory.geyser.reheated_at },
            "changes": changes.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
            "rows": {
                "geyser": row(&result.status.geyser.row),
                "water": row(&result.status.water),
                "hotplate": row(&result.status.hotplate.row),
                "ev": row(&result.status.ev.row),
            },
            "command": result.geyser_command.as_ref().map(|c| serde_json::json!({
                "intent": format!("{:?}", c.intent),
                "buttons": c.buttons.iter().map(|b| b.as_str()).collect::<Vec<_>>(),
            })),
            "asks": result.asks.iter().map(|a| &a.body).collect::<Vec<_>>(),
            "notify": result.notify.iter().map(|(_, body)| body).collect::<Vec<_>>(),
            "answer": input.answer.map(|a| format!("{a:?}")),
            "edits": input.edits.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>(),
            "request": input.geyser_request.map(|r| format!("{r:?}")),
            "ai": format!("{:?}", result.status.ai.mode),
        }));
    }
    result
}

fn hotplate_levels(top: f64) -> Vec<f64> {
    let top = top.clamp(HOTPLATE_MIN_W, HOTPLATE_MAX_W);
    std::iter::once(top)
        .chain(HOTPLATE_SETTINGS.into_iter().filter(|level| *level < top - 1.0 && *level >= HOTPLATE_MIN_W))
        .collect()
}

fn geyser_first_reason(input: &LoadsInput) -> Option<String> {
    let config = input.config;
    let today = input.now.date();
    if config.geyser_first_weekdays.contains(&today.weekday().num_days_from_monday()) {
        return Some(format!("{}: everyone bathes", today.format("%A")));
    }
    let forecast = input.forecast?;
    let low = forecast
        .night_min_temp(today - Duration::days(1))
        .or_else(|| forecast.day(today).and_then(|day| day.temp_min_c))?;
    (low < config.geyser_first_below_c).then(|| format!("last night went down to {low:.0}°C"))
}

fn answer_hotplate(memory: &mut LoadsMemory, cooking: bool, now: NaiveDateTime, house_w: f64) {
    let hotplate = &mut memory.hotplate;
    if cooking {
        let run = match hotplate.asking.take() {
            Some(ask) => HotplateRun { since: ask.at, base_w: ask.base_w, believed_at: ask.at, confirmed: true },
            None => HotplateRun { since: now, base_w: house_w, believed_at: now, confirmed: true },
        };
        hotplate.running = Some(run);
        hotplate.quiet_since = None;
    } else {
        if let Some(change) = hotplate.asking.take().and_then(|ask| ask.change.map(|change| (change, ask.at))) {
            memory.detector.apply(&change.0, change.1);
        }
        hotplate.declined_at = Some(now);
    }
}

fn end_hotplate(memory: &mut LoadsMemory) {
    memory.hotplate.running = None;
    memory.hotplate.asking = None;
    memory.hotplate.quiet_since = None;
    memory.ai.next_at = None;
}

/// A question nobody answered goes away with the load; a session ends once
/// the hotplate's share of the load has stayed near nothing for a while.
fn track_hotplate_off(memory: &mut LoadsMemory, hotplate_w: Option<f64>, now: NaiveDateTime) {
    let off = hotplate_w.is_some_and(|watts| watts < HOTPLATE_OFF_W);
    if off && memory.hotplate.asking.is_some() {
        memory.hotplate.asking = None;
    }
    if memory.hotplate.running.is_none() || !off {
        memory.hotplate.quiet_since = None;
        return;
    }
    let since = *memory.hotplate.quiet_since.get_or_insert(now);
    if now - since >= Duration::minutes(HOTPLATE_OFF_MINUTES) {
        memory.hotplate.running = None;
        memory.hotplate.quiet_since = None;
    }
}

/// Which elements a load reading means: (800 W element, 1200 W element).
fn geyser_elements_status(belief: &ElementsBelief, heating: bool) -> Option<GeyserElements> {
    let (low, high) = belief.selected?;
    let state = |on: bool| match (on, heating) {
        (true, true) => ElementState::Heating,
        (true, false) => ElementState::KeepWarm,
        (false, _) => ElementState::Off,
    };
    Some(GeyserElements { low: state(low), high: state(high), unsure: belief.unsure, why: belief.why.clone(), at: belief.at })
}

fn geyser_elements(watts: f64, config: &AutomationConfig) -> Option<(bool, bool)> {
    let (low, high) = (config.geyser_low_watts, config.geyser_high_watts);
    [((true, false), low), ((false, true), high), ((true, true), low + high)]
        .into_iter()
        .map(|(elements, level)| (elements, (watts - level).abs()))
        .filter(|(_, gap)| *gap <= (0.2 * watts).max(150.0))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(elements, _)| elements)
}

/// The element toggles from what is on now to the target level. The element
/// being added goes first, so the geyser never sits with both off.
fn geyser_toggles(now_on: (bool, bool), target: f64, config: &AutomationConfig) -> Option<Vec<Button>> {
    let want = geyser_elements(target, config)?;
    let mut buttons = Vec::new();
    for (wanted, on, button) in [(want.0, now_on.0, Button::W800), (want.1, now_on.1, Button::W1200)] {
        if wanted && !on {
            buttons.push(button);
        }
    }
    for (wanted, on, button) in [(want.0, now_on.0, Button::W800), (want.1, now_on.1, Button::W1200)] {
        if !wanted && on {
            buttons.push(button);
        }
    }
    (!buttons.is_empty()).then_some(buttons)
}

/// The presses that switch every element that is on now off; the geyser
/// itself stays on, drawing nothing.
fn elements_off(now_on: (bool, bool)) -> Option<Vec<Button>> {
    let buttons: Vec<Button> = [(now_on.0, Button::W800), (now_on.1, Button::W1200)]
        .into_iter()
        .filter_map(|(on, button)| on.then_some(button))
        .collect();
    (!buttons.is_empty()).then_some(buttons)
}

fn names(buttons: &[Button]) -> String {
    buttons.iter().map(|button| button.label()).collect::<Vec<_>>().join(" and ")
}

fn level_matches(running: &Running, watts: f64, config: &AutomationConfig) -> bool {
    geyser_elements(running.watts, config).is_some() && geyser_elements(running.watts, config) == geyser_elements(watts, config)
}

/// A geyser run is watched for its first minutes; at the end the readings are
/// summed up and the AI is asked again with them. Returns when the watch ends
/// while it is still on.
fn watch_geyser(control: &mut GeyserControl, running: Option<&Running>, input: &LoadsInput, sunset: NaiveDateTime) -> Option<NaiveDateTime> {
    let now = input.now;
    let Some(running) = running else {
        control.watch = None;
        if control.pending.is_none() {
            control.night_hold = false;
        }
        return None;
    };
    let until = running.since + Duration::minutes(GEYSER_WATCH_MINUTES);
    if control.watch.as_ref().is_none_or(|watch| watch.since != running.since) {
        control.watch = (input.daytime && now < sunset && now < until).then_some(GeyserWatch {
            since: running.since,
            watts: running.watts,
            started_at: running.since,
            until,
            ended_at: None,
            summary: None,
            announced: false,
        });
    }
    let watch = control.watch.as_mut()?;
    if watch.ended_at.is_none() && now >= watch.until {
        watch.ended_at = Some(now);
        watch.summary = watch_summary(input.recent, watch.started_at);
    }
    watch.ended_at.is_none().then_some(watch.until)
}

fn watch_summary(recent: &[Sample], from: NaiveDateTime) -> Option<String> {
    let window: Vec<&Sample> = recent.iter().filter(|sample| sample.at >= from).collect();
    let mean = |values: Vec<f64>| (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
    let pv = mean(window.iter().filter_map(|sample| sample.pv_w).collect())?;
    let load = mean(window.iter().filter_map(|sample| sample.load_w).collect())?;
    let battery = mean(window.iter().filter_map(|sample| Some(sample.battery_a? * sample.battery_v?)).collect());
    let mut parts = vec![format!("sun {:.1} kW", pv / 1000.0), format!("house {:.1} kW with the geyser", load / 1000.0)];
    match battery {
        Some(watts) if watts >= 100.0 => parts.push(format!("battery charging {watts:.0} W")),
        Some(watts) if watts <= -100.0 => parts.push(format!("battery draining {:.0} W", -watts)),
        Some(_) => parts.push("battery steady".into()),
        None => {}
    }
    if let (Some(first), Some(last)) = (window.iter().find_map(|sample| sample.soc), window.iter().rev().find_map(|sample| sample.soc)) {
        parts.push(format!("{first:.0}% → {last:.0}%"));
    }
    if window.last().and_then(|sample| sample.mode.as_deref()) == Some("Solar") {
        parts.push("the grid is carrying the house (Solar mode)".into());
    }
    Some(parts.join(", "))
}

/// Once the watch is over and the plan has caught up with it, tells the
/// family what the readings were and what happens now.
fn watch_decision(
    control: &mut GeyserControl,
    decided_at: Option<NaiveDateTime>,
    verdict: &Verdict,
    ai_available: bool,
    auto: bool,
    now: NaiveDateTime,
    config: &AutomationConfig,
) -> Option<String> {
    let watch = control.watch.as_mut().filter(|watch| !watch.announced)?;
    let ended = watch.ended_at?;
    let caught_up = !ai_available || decided_at.is_some_and(|at| at >= ended) || now - ended >= Duration::minutes(GEYSER_DECISION_WAIT_MINUTES);
    if !caught_up {
        return None;
    }
    watch.announced = true;
    let decision = match (verdict.phase, verdict.watts) {
        (LoadPhase::Stop, Some(level)) => format!("turn it down to {}", level_text(level, config)),
        (LoadPhase::Stop, None) => "switch it off".to_string(),
        _ => "keep it on".to_string(),
    };
    let readings = watch.summary.clone().unwrap_or_else(|| "no readings came in".into());
    Some(format!(
        "Watched {GEYSER_WATCH_MINUTES} min at {}: {readings}. {}: {decision}. {}",
        level_text(watch.watts, config),
        if auto { "Decision" } else { "Suggestion" },
        verdict.note
    ))
}

/// The family pressing On or Off in the app: worked out from the load the
/// same way, then checked like any other press.
fn manual_geyser(
    memory: &mut LoadsMemory,
    request: GeyserIntent,
    running: Option<&Running>,
    daytime: bool,
    config: &AutomationConfig,
    notify: &mut Vec<(&'static str, String)>,
) -> Option<GeyserCommand> {
    let control = &mut memory.geyser;
    if matches!(request, GeyserIntent::On(_)) && running.is_none() && control.unknown_w > GEYSER_UNKNOWN_LOAD_W {
        notify.push((
            GEYSER_TITLE,
            format!(
                "Nothing pressed: about {:.0} W is on that the app can't place. If that's the geyser it is already heating, and the element buttons would switch it off. Tell the app with Change.",
                control.unknown_w
            ),
        ));
        return None;
    }
    let request = match request {
        GeyserIntent::On(target) if target > control.room_w + 1.0 => {
            match geyser_levels(config).into_iter().find(|level| *level <= control.room_w) {
                Some(level) => {
                    notify.push((GEYSER_TITLE, format!("{target:.0} W would overload the inverter with what else is on: {level:.0} W instead.")));
                    GeyserIntent::On(level)
                }
                None => {
                    notify.push((GEYSER_TITLE, "Nothing pressed: with what else is on (the hotplate?), the geyser would overload the inverter.".into()));
                    return None;
                }
            }
        }
        request => request,
    };
    control.pending = None;
    control.failed_on = None;
    let buttons = match (request, running) {
        (GeyserIntent::On(target), None) => geyser_toggles((false, false), target, config),
        (GeyserIntent::On(target), Some(r)) => geyser_elements(r.watts, config).and_then(|on| geyser_toggles(on, target, config)),
        (GeyserIntent::Off, Some(r)) => geyser_elements(r.watts, config).and_then(elements_off),
        (GeyserIntent::Off, None) => None,
    };
    let Some(buttons) = buttons else {
        let message = match (request, running) {
            (GeyserIntent::On(target), Some(r)) if level_matches(r, target, config) => format!("The geyser is already on at {target:.0} W."),
            (_, Some(r)) => format!("The geyser draws {:.0} W, which isn't a level the app knows. Set it on the panel.", r.watts),
            _ => "The geyser isn't drawing anything, so its elements are already off. Nothing pressed.".to_string(),
        };
        notify.push((GEYSER_TITLE, message));
        return None;
    };
    control.night_hold = matches!(request, GeyserIntent::On(_)) && !daytime;
    let after = match request {
        GeyserIntent::On(_) if daytime => format!(" Then it watches the sun and battery for {GEYSER_WATCH_MINUTES} minutes and decides."),
        GeyserIntent::On(watts) if watts > config.geyser_low_watts.min(config.geyser_high_watts) + 1.0 => {
            " It stays on tonight until the water is hot or you switch it off; above 800 W it can trip the oven boost.".to_string()
        }
        GeyserIntent::On(_) => " It stays on tonight until the water is hot or you switch it off.".to_string(),
        GeyserIntent::Off => String::new(),
    };
    let what = match request {
        GeyserIntent::On(watts) => format!("on at {watts:.0} W"),
        GeyserIntent::Off => "off".into(),
    };
    notify.push((GEYSER_TITLE, format!("You asked for the geyser {what}: pressing {}.{after}", names(&buttons))));
    Some(GeyserCommand { intent: request, buttons })
}

/// With the ESP32 in charge, the plan becomes button presses. The geyser
/// stays powered on and only its 800 W and 1200 W elements are toggled, so
/// nothing drawing means no element is on (or the thermostat has cut out).
/// Presses are worked out from what the inverter sees drawing, never from
/// memory, and checked on a reading taken after them. A switch-on that still
/// draws nothing is not repeated: the water is probably hot, and control goes
/// back to reminders for the day; a geyser seen drawing when it should be off
/// is still switched off.
/// After an On press that left nothing drawing: the load fell by about the
/// pressed elements' power, so they were on already and the press toggled
/// them off. Hands back the fall and the same buttons to press again.
fn switched_off_by_press(control: &GeyserControl, config: &AutomationConfig) -> Option<(f64, Vec<Button>)> {
    let (before, buttons) = control.on_sent.as_ref()?;
    let pressed_w: f64 = buttons
        .iter()
        .map(|button| match button {
            Button::W800 => config.geyser_low_watts,
            Button::W1200 => config.geyser_high_watts,
            _ => 0.0,
        })
        .sum();
    let fell = before - control.last_load_w?;
    (pressed_w > 0.0 && (fell - pressed_w).abs() <= tolerance(pressed_w)).then(|| (fell, buttons.clone()))
}

fn geyser_command(
    memory: &mut LoadsMemory,
    verdict: &Verdict,
    running: Option<&Running>,
    control_by: Control,
    now: NaiveDateTime,
    config: &AutomationConfig,
    notify: &mut Vec<(&'static str, String)>,
) -> Option<GeyserCommand> {
    let control = &mut memory.geyser;
    if control.urgent {
        control.pending = None;
        control.last_sent_at = None;
    }
    if let Some((intent, sent_at, attempts)) = control.pending {
        let fresh = control.reading_at.unwrap_or(now) >= sent_at + Duration::seconds(GEYSER_SETTLE_SECONDS);
        let waited = fresh && now - sent_at >= Duration::minutes(GEYSER_COMMAND_WAIT_MINUTES);
        match (intent, running) {
            (GeyserIntent::Off, None) => {
                control.pending = None;
                control.checked(now, CheckState::Confirmed, "Nothing is drawing on the load: its elements are off.".into());
                notify.push((GEYSER_TITLE, "Confirmed: the geyser's elements are off.".into()));
            }
            (GeyserIntent::On(target), Some(r)) if level_matches(r, target, config) => {
                control.pending = None;
                control.checked(now, CheckState::Confirmed, format!("The load shows it drawing {:.0} W.", r.watts));
                notify.push((GEYSER_TITLE, format!("Confirmed: the geyser is on at {target:.0} W (drawing {:.0} W).", r.watts)));
            }
            _ if !waited => return None,
            (GeyserIntent::Off, Some(r)) if attempts < 2 => {
                if let Some(buttons) = geyser_elements(r.watts, config).and_then(elements_off) {
                    notify.push((GEYSER_TITLE, format!("The geyser is still drawing {:.0} W — pressing {} again.", r.watts, names(&buttons))));
                    return Some(GeyserCommand { intent, buttons });
                }
            }
            (GeyserIntent::On(target), Some(r)) if attempts < 3 => {
                if let Some(buttons) = geyser_elements(r.watts, config).and_then(|on| geyser_toggles(on, target, config)) {
                    notify.push((GEYSER_TITLE, format!("The geyser is at {:.0} W — switching it to {target:.0} W.", r.watts)));
                    return Some(GeyserCommand { intent, buttons });
                }
                control.pending = None;
                control.checked(now, CheckState::Confirmed, format!("On, drawing {:.0} W; that isn't a level the app knows.", r.watts));
            }
            (GeyserIntent::On(target), None) if control.reheat_since.is_some() => {
                control.pending = None;
                control.reheat_since = None;
                control.reheated_at = Some(now);
                control.idle_elements = geyser_elements(target, config);
                control.checked(now, CheckState::NotConfirmed, "No draw: its thermostat isn't calling for heat, so the water is still hot.".into());
                notify.push((
                    GEYSER_TITLE,
                    "The geyser didn't draw after the top-up press: its thermostat says the water is still hot. Switching the elements back off so it doesn't heat tonight.".into(),
                ));
                return None;
            }
            (GeyserIntent::On(target), None) if attempts < 2 && switched_off_by_press(control, config).is_some() => {
                let (fell, buttons) = switched_off_by_press(control, config)?;
                notify.push((
                    GEYSER_TITLE,
                    format!(
                        "The load fell by {fell:.0} W after pressing {}: that element was already heating without the app knowing, so the press switched it off. Pressing it again to switch it back on.",
                        names(&buttons)
                    ),
                ));
                return Some(GeyserCommand { intent: GeyserIntent::On(target), buttons });
            }
            (GeyserIntent::On(_), None) => {
                control.pending = None;
                control.failed_on = Some(now.date());
                control.checked(
                    now,
                    CheckState::NotConfirmed,
                    "No draw on the load: the water is probably already hot, or the geyser didn't catch the signal.".into(),
                );
                notify.push((
                    GEYSER_TITLE,
                    "The ESP32 pressed the element buttons but the geyser isn't drawing power — the water is probably already hot, or it didn't catch the signal. Check the panel; back to reminders for today.".into(),
                ));
                return None;
            }
            _ => {}
        }
        if control.pending.is_some_and(|(_, at, _)| at == sent_at) && waited {
            control.pending = None;
            control.failed_on = Some(now.date());
            let message = match (intent, running) {
                (GeyserIntent::Off, Some(r)) => format!("The geyser is still drawing {:.0} W after two tries — switch its elements off by hand. Back to reminders for today.", r.watts),
                (GeyserIntent::On(target), Some(r)) => format!("The geyser is at {:.0} W, not {target:.0} W, after three tries — set it by hand. Back to reminders for today.", r.watts),
                _ => "The geyser didn't do what the ESP32 asked — check the panel. Back to reminders for today.".into(),
            };
            let seen = match running {
                Some(r) => format!("Still drawing {:.0} W after the presses.", r.watts),
                None => "The load didn't show the change.".into(),
            };
            control.checked(now, CheckState::NotConfirmed, seen);
            notify.push((GEYSER_TITLE, message));
            return None;
        }
        if control.pending.is_some() {
            return None;
        }
    }
    if control_by == Control::VerifyOnly || control.last_sent_at.is_some_and(|at| now - at < Duration::minutes(GEYSER_COMMAND_GAP_MINUTES)) {
        return None;
    }
    let full = control_by == Control::Full;
    if let Some(selected) = control.idle_elements.take().filter(|_| full && running.is_none()) {
        let buttons = elements_off(selected)?;
        let level = [(selected.0, config.geyser_low_watts), (selected.1, config.geyser_high_watts)].iter().filter(|(on, _)| *on).map(|(_, w)| w).sum();
        control.idle_check = control.last_load_w.map(|load| (load, level));
        notify.push((
            GEYSER_TITLE,
            format!("The thermostat cut the geyser out, so the water is hot: pressing {} so it doesn't heat again on its own.", names(&buttons)),
        ));
        return Some(GeyserCommand { intent: GeyserIntent::Off, buttons });
    }
    let command = match (verdict.phase, running) {
        (LoadPhase::Ready, None) if full && control.unknown_w <= GEYSER_UNKNOWN_LOAD_W => GeyserCommand {
            intent: GeyserIntent::On(verdict.watts?),
            buttons: geyser_toggles((false, false), verdict.watts?, config)?,
        },
        (LoadPhase::Stop, Some(r)) if !full && verdict.watts.is_none() => {
            GeyserCommand { intent: GeyserIntent::Off, buttons: geyser_elements(r.watts, config).and_then(elements_off)? }
        }
        (LoadPhase::Stop, Some(r)) if full => {
            let on = geyser_elements(r.watts, config)?;
            match verdict.watts {
                Some(level) => GeyserCommand { intent: GeyserIntent::On(level), buttons: geyser_toggles(on, level, config)? },
                None => GeyserCommand { intent: GeyserIntent::Off, buttons: elements_off(on)? },
            }
        }
        (LoadPhase::Running, Some(r)) if full && verdict.watts.is_some_and(|watts| watts > r.watts + 1.0) => {
            let level = verdict.watts?;
            GeyserCommand { intent: GeyserIntent::On(level), buttons: geyser_toggles(geyser_elements(r.watts, config)?, level, config)? }
        }
        _ => return None,
    };
    if let (GeyserIntent::On(level), Some(r)) = (command.intent, running) {
        if level < r.watts - 1.0 {
            control.stepped_down_at = Some(now);
        }
    }
    if matches!((command.intent, running), (GeyserIntent::Off, Some(_))) {
        control.stepped_down_at = Some(now);
    }
    let message = match (command.intent, running) {
        (GeyserIntent::On(watts), None) => format!("Switching the geyser on at {watts:.0} W ({}). {}", names(&command.buttons), verdict.note),
        (GeyserIntent::On(watts), Some(r)) if watts > r.watts => format!("Turning the geyser up to {watts:.0} W ({}). {}", names(&command.buttons), verdict.note),
        (GeyserIntent::On(watts), Some(_)) => format!("Turning the geyser down to {watts:.0} W ({}). {}", names(&command.buttons), verdict.note),
        (GeyserIntent::Off, _) => format!("Switching the geyser's elements off ({}). {}", names(&command.buttons), verdict.note),
    };
    notify.push((GEYSER_TITLE, message));
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::agent::{LoadsDecision, LoadsNotes, LoadsRun, LoadsRunning};
    use crate::automation::weather::{DayWeather, HourWeather};

    fn decision(running: &[(&str, f64, &str)], plan: &[(&str, &str, &str, f64)]) -> LoadsDecision {
        LoadsDecision {
            running: running.iter().map(|(load, watts, since)| LoadsRunning { load: (*load).into(), watts: *watts, since: (*since).into() }).collect(),
            sure: true,
            question: String::new(),
            plan: plan
                .iter()
                .map(|(load, start, end, watts)| LoadsRun { load: (*load).into(), start: (*start).into(), end: (*end).into(), watts: *watts })
                .collect(),
            scooter_first: false,
            notes: LoadsNotes { geyser: "Geyser on at 1200 W — the sun carries it until 2 PM.".into(), ..LoadsNotes::default() },
            recheck_minutes: 15.0,
            reason: "The load jumped by 1200 W at 10:57 and stayed: the geyser.".into(),
            model: Some("test-model".into()),
        }
    }

    fn ai_step<'a>(now: NaiveDateTime, load: f64, sun: &'a Forecast, config: &'a AutomationConfig) -> LoadsInput<'a> {
        LoadsInput { ai_available: true, ..input(now, 99.0, 1380.0, load, Some(sun), config) }
    }

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn config() -> AutomationConfig {
        AutomationConfig { pv_array_watts: 4000.0, hotplate_enabled: false, geyser_winter_only: false, ..AutomationConfig::default() }
    }

    fn sunny(peak: f64) -> Forecast {
        let hourly = (0..24)
            .map(|hour| {
                let radiation = if (8..=17).contains(&hour) { peak * (1.0 - ((hour as f64 - 12.5) / 5.0).powi(2)).max(0.0) } else { 0.0 };
                HourWeather { at: at(hour, 0), temp_c: None, cloud_pct: Some(10.0), radiation_w_m2: Some(radiation) }
            })
            .collect();
        let today = DayWeather { date: at(0, 0).date(), radiation_kwh_m2: None, sunshine_h: None, rain_prob_pct: None, temp_min_c: Some(24.0) };
        Forecast { hourly, daily: vec![today] }
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
            pv_curve: &[],
            sky_factor: None,
            typical_load: &|_| Some(300.0),
            tonight_needs_soc: Some(45.0),
            config,
            water_pending: false,
            geyser_done_on: None,
            ev_skip_on: None,
            hotplate_on: None,
            hotplate_skip_on: None,
            ev_checked_on: None,
            answer: None,
            edits: &[],
            ai_available: false,
            recent: &[],
            ev_reading: None,
            ev_reader_busy: false,
            remote_ready: false,
            geyser_request: None,
            geyser_heard_at: None,
            geyser_heard_button: None,
            battery_w: None,
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

    fn cold(mut forecast: Forecast) -> Forecast {
        forecast.daily[0].temp_min_c = Some(12.0);
        forecast
    }

    #[test]
    fn a_cold_dark_day_still_heats_the_geyser_with_the_grid_topping_up() {
        let config = config();
        let sky = cold(sunny(120.0));
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(10, 30), 55.0, 150.0, 300.0, Some(&sky), &config));
        let geyser = &result.status.geyser.row;
        assert_ne!(geyser.phase, LoadPhase::Skipped);
        assert!(geyser.note.as_deref().unwrap().contains("grid tops"), "{:?}", geyser.note);
        assert!(result.status.plan.iter().any(|slot| slot.kind == LoadKind::Geyser && slot.topped_up));
        assert!(result.status.geyser_required.as_deref().unwrap().contains("12°C"));
        assert!(result.status.grid_kwh.is_some());
    }

    #[test]
    fn in_winter_the_geyser_heats_every_day_even_on_a_mild_dark_one() {
        let config = AutomationConfig { geyser_winter_only: true, ..config() };
        let sky = sunny(120.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(10, 30), 55.0, 150.0, 300.0, Some(&sky), &config));
        assert!(result.status.geyser_required.as_deref().unwrap().contains("winter"));
        assert!(result.status.plan.iter().any(|slot| slot.kind == LoadKind::Geyser && slot.topped_up));
        assert_ne!(result.status.geyser.row.phase, LoadPhase::Skipped);

        let any_season = AutomationConfig { geyser_winter_only: false, ..config };
        let result = step(&mut LoadsMemory::default(), &input(at(10, 30), 55.0, 150.0, 300.0, Some(&sky), &any_season));
        assert!(result.status.geyser_required.is_none());
    }

    #[test]
    fn outside_winter_the_app_never_plans_or_presses_the_geyser() {
        let config = AutomationConfig { geyser_winter_only: true, geyser_auto_control: true, geyser_esp32_host: "esp".into(), ..config() };
        assert!(config.geyser_planned_on(at(10, 30).date()));
        let may = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        assert!(!config.geyser_planned_on(may));
        let sky = sunny(900.0);
        let mut memory = LoadsMemory::default();
        let mut result = LoadsStep::default();
        for minute in [0, 5, 10, 15, 20] {
            let now = may.and_hms_opt(11, minute, 0).unwrap();
            result = step(&mut memory, &input(now, 100.0, 2400.0, 300.0, Some(&sky), &config));
            assert!(result.geyser_command.is_none());
        }
        assert_eq!(result.status.geyser.row.phase, LoadPhase::Off);
        assert!(result.status.geyser_required.is_none());
        assert!(!result.status.plan.iter().any(|slot| slot.kind == LoadKind::Geyser));
    }

    #[test]
    fn a_cold_sunny_day_heats_the_geyser_on_the_sun_alone() {
        let config = config();
        let sky = cold(sunny(800.0));
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(10, 0), 90.0, 1500.0, 300.0, Some(&sky), &config));
        assert!(result.status.plan.iter().any(|slot| slot.kind == LoadKind::Geyser && !slot.topped_up));
        assert!(!result.status.grid_now);
    }

    #[test]
    fn a_required_scooter_on_the_charger_is_not_unplugged_on_a_dark_day() {
        let config = config();
        let sky = sunny(120.0);
        let mut memory = LoadsMemory::default();
        let reading = Some(EvReading { soc: 60.0, at: at(9, 0), live: false });
        let tick = |memory: &mut LoadsMemory, now: NaiveDateTime, load: f64| {
            step(memory, &LoadsInput { ev_reading: reading.clone(), ..input(now, 55.0, 150.0, load, Some(&sky), &config) })
        };
        tick(&mut memory, at(10, 0), 300.0);
        tick(&mut memory, at(10, 2), 300.0 + config.ev_charger_watts);
        let result = tick(&mut memory, at(10, 4), 300.0 + config.ev_charger_watts);
        let ev = &result.status.ev.row;
        assert_eq!(ev.phase, LoadPhase::Running, "{:?}", ev.note);
        assert!(ev.note.as_deref().unwrap().contains("grid tops"), "{:?}", ev.note);
        assert!(result.status.grid_now);
        assert!(memory.grid_now().unwrap().contains("scooter"));
    }

    #[test]
    fn the_battery_lends_to_a_required_load_when_the_sun_refills_it_later() {
        let config = config();
        let sky = cold(sunny(900.0));
        let mut memory = LoadsMemory::default();
        let reading = Some(EvReading { soc: 60.0, at: at(8, 0), live: false });
        let tick = |memory: &mut LoadsMemory, now: NaiveDateTime, load: f64| {
            step(memory, &LoadsInput { ev_reading: reading.clone(), ..input(now, 90.0, 300.0, load, Some(&sky), &config) })
        };
        tick(&mut memory, at(8, 30), 300.0);
        tick(&mut memory, at(8, 32), 300.0 + config.ev_charger_watts);
        let result = tick(&mut memory, at(8, 34), 300.0 + config.ev_charger_watts);
        assert_eq!(result.status.ev.row.phase, LoadPhase::Running, "{:?}", result.status.ev.row.note);
        assert!(!result.status.grid_now, "a sunny day ahead refills what the battery lends now");
    }

    #[test]
    fn the_loads_leave_the_battery_as_full_as_the_sun_alone() {
        let config = config();
        let sun = sunny(600.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(10, 15), 50.0, 1200.0, 300.0, Some(&sun), &config));
        let target = result.status.sunset_target_soc.unwrap();
        assert!(result.status.projected_sunset_soc.unwrap() >= target - 0.5);
        assert!(result.status.full_by.is_some());
        assert!(!result.status.plan.is_empty());
    }

    fn cooking() -> AutomationConfig {
        AutomationConfig { hotplate_enabled: true, ..config() }
    }

    #[test]
    fn hotplate_levels_step_down_from_the_top() {
        assert_eq!(hotplate_levels(1000.0), vec![1000.0, 800.0, 400.0]);
        assert_eq!(hotplate_levels(1730.0)[..3], [1730.0, 1600.0, 1400.0]);
    }

    #[test]
    fn an_unasked_jump_while_cooking_is_likely_asks_about_the_hotplate() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        let asked = step(&mut memory, &input(at(11, 2), 100.0, 2300.0, 2300.0, Some(&sun), &config));
        assert_eq!(asked.status.hotplate.row.phase, LoadPhase::Ask);
        assert_eq!(asked.status.hotplate.asking_w, Some(2000.0));
        assert!(asked.asks.iter().any(|ask| ask.title == HOTPLATE_TITLE && ask.about == AskAbout::Loads && ask.body.contains("is it the hotplate")));
        assert!(memory.detector.running(LoadKind::Geyser).is_none());
    }

    #[test]
    fn no_hands_the_jump_back_to_the_geyser() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 2300.0, 2300.0, Some(&sun), &config));
        let mut answer = input(at(11, 4), 100.0, 2300.0, 2300.0, Some(&sun), &config);
        answer.answer = Some(LoadsAnswer::No);
        let result = step(&mut memory, &answer);
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(2000.0));
        assert_ne!(result.status.hotplate.row.phase, LoadPhase::Ask);
    }

    #[test]
    fn yes_tracks_the_hotplate_and_asks_to_turn_it_down_when_the_sun_falls_short() {
        let config = cooking();
        let sun = sunny(500.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 2500.0, 2500.0, Some(&sun), &config));
        let mut answer = input(at(11, 4), 100.0, 1400.0, 2500.0, Some(&sun), &config);
        answer.answer = Some(LoadsAnswer::Yes);
        answer.hotplate_on = Some(at(11, 4).date());
        let cooking = step(&mut memory, &answer);
        let hotplate = &cooking.status.hotplate;
        assert_eq!(hotplate.row.running_w, Some(2200.0));
        assert_eq!(hotplate.row.phase, LoadPhase::Stop);
        assert!(hotplate.row.watts.unwrap() < 2200.0);
        assert!(hotplate.row.note.as_deref().unwrap().contains("Turn the hotplate down"));
        let level_up = step(&mut memory, &{
            let mut next = input(at(11, 6), 100.0, 1400.0, 3100.0, Some(&sun), &config);
            next.hotplate_on = Some(at(11, 4).date());
            next
        });
        assert!(memory.detector.running(LoadKind::Ev).is_none());
        assert_eq!(level_up.status.hotplate.row.running_w, Some(2800.0));
    }

    #[test]
    fn the_session_ends_once_the_hotplate_has_been_off_a_while() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let today = at(11, 0).date();
        let tick = |memory: &mut LoadsMemory, now: NaiveDateTime, load: f64, answer: Option<LoadsAnswer>| {
            let mut next = input(now, 100.0, 1500.0, load, Some(&sun), &config);
            next.answer = answer;
            next.hotplate_on = (now > at(11, 2)).then_some(today);
            step(memory, &next)
        };
        tick(&mut memory, at(11, 0), 300.0, None);
        tick(&mut memory, at(11, 2), 1800.0, None);
        tick(&mut memory, at(11, 4), 1800.0, Some(LoadsAnswer::Yes));
        tick(&mut memory, at(12, 0), 300.0, None);
        assert!(memory.hotplate.running.is_some());
        let done = tick(&mut memory, at(12, 11), 300.0, None);
        assert!(memory.hotplate.running.is_none());
        assert_eq!(done.status.hotplate.row.phase, LoadPhase::Done);
    }

    #[test]
    fn a_cooking_day_plans_a_session_and_the_day_after_does_not() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let today = step(&mut memory, &input(at(9, 30), 100.0, 1500.0, 300.0, Some(&sun), &config));
        assert!(today.status.hotplate.cooking_day);
        let session = today.status.plan.iter().find(|s| s.kind == LoadKind::Hotplate).unwrap();
        assert_eq!(session.watts, config.hotplate_watts);
        assert_eq!(session.end - session.start, Duration::minutes(150));
        assert!(session.start >= at(10, 0) && session.end <= at(15, 0));

        let mut memory = LoadsMemory::default();
        let mut after = input(at(9, 30), 100.0, 1500.0, 300.0, Some(&sun), &config);
        after.hotplate_on = Some(at(9, 30).date() - Duration::days(1));
        let after = step(&mut memory, &after);
        assert!(!after.status.hotplate.cooking_day);
        assert!(after.status.plan.iter().all(|s| s.kind != LoadKind::Hotplate));
        assert!(after.status.hotplate.row.note.as_deref().unwrap().contains("next one"));
    }

    #[test]
    fn fridays_and_cold_nights_put_the_geyser_first() {
        let config = cooking();
        let sun = sunny(800.0);
        let tuesday = input(at(9, 30), 100.0, 1500.0, 300.0, Some(&sun), &config);
        assert_eq!(geyser_first_reason(&tuesday), None);
        let friday = LoadsInput { now: at(9, 30) + Duration::days(3), ..input(at(9, 30), 100.0, 1500.0, 300.0, Some(&sun), &config) };
        assert_eq!(geyser_first_reason(&friday).as_deref(), Some("Friday: everyone bathes"));
        let mut cold = sunny(800.0);
        for hour in &mut cold.hourly {
            hour.temp_c = Some(7.0);
        }
        cold.hourly.extend((0..24).map(|hour| HourWeather {
            at: at(hour, 0) - Duration::days(1),
            temp_c: Some(7.0),
            cloud_pct: None,
            radiation_w_m2: None,
        }));
        let chilly = input(at(9, 30), 100.0, 1500.0, 300.0, Some(&cold), &config);
        assert!(geyser_first_reason(&chilly).unwrap().contains("7°C"));
    }

    #[test]
    fn the_ai_is_asked_at_the_start_and_again_on_a_big_change_in_the_load() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let first = step(&mut memory, &ai_step(at(11, 25), 1370.0, &sun, &config));
        let (input, checker) = first.ai_request.expect("asked at the start");
        assert_eq!(input.load_w, Some(1370.0));
        assert_eq!(input.time, "11:25");
        let answer = decision(&[("geyser", 1200.0, "10:57")], &[("geyser", "10:57", "13:30", 1200.0)]);
        memory.apply_ai(checker.check(&answer).unwrap(), &answer, Some(1370.0), at(11, 25));
        assert!(step(&mut memory, &ai_step(at(11, 27), 1390.0, &sun, &config)).ai_request.is_none());
        assert!(step(&mut memory, &ai_step(at(11, 34), 2200.0, &sun, &config)).ai_request.is_none(), "within 10 minutes of the last ask");
        assert!(step(&mut memory, &ai_step(at(11, 35), 2200.0, &sun, &config)).ai_request.is_some());
        assert!(step(&mut memory, &ai_step(at(11, 50), 2200.0, &sun, &config)).ai_request.is_none());
        assert!(step(&mut memory, &ai_step(at(12, 5), 2200.0, &sun, &config)).ai_request.is_some());
    }

    #[test]
    fn the_ai_cant_put_the_geyser_on_when_nothing_was_pressed_since_its_elements_went_off() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut input = ai_step(at(12, 50), 1100.0, &sun, &config);
        input.remote_ready = true;
        let (ai_input, checker) = step(&mut memory, &input).ai_request.unwrap();
        assert!(serde_json::to_string(&ai_input).unwrap().contains("it is not running"));
        let wrong = checker.check(&decision(&[("geyser", 800.0, "12:26")], &[])).unwrap_err();
        assert!(wrong.iter().any(|problem| problem.contains("can't be on")), "{wrong:?}");
        assert!(checker.check(&decision(&[("hotplate", 800.0, "12:26")], &[])).is_ok());
    }

    #[test]
    fn the_checker_wants_the_running_loads_to_explain_the_house_load() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &ai_step(at(11, 25), 1370.0, &sun, &config)).ai_request.unwrap();
        let nothing = checker.check(&decision(&[], &[])).unwrap_err();
        assert!(nothing[0].contains("isn't listed is on"), "{nothing:?}");
        let too_much = checker.check(&decision(&[("geyser", 2000.0, "10:57")], &[])).unwrap_err();
        assert!(too_much[0].contains("more than the 1370 W"), "{too_much:?}");
        let odd_level = checker.check(&decision(&[("geyser", 1500.0, "10:57")], &[])).unwrap_err();
        assert!(odd_level[0].contains("exactly"), "{odd_level:?}");
        assert!(checker.check(&decision(&[("geyser", 1200.0, "10:57")], &[("geyser", "10:57", "13:30", 1200.0)])).is_ok());
    }

    #[test]
    fn the_checker_holds_the_ai_to_the_windows_and_the_battery_rules() {
        let config = cooking();
        let sun = sunny(500.0);
        let mut memory = LoadsMemory::default();
        let mut late = ai_step(at(11, 25), 300.0, &sun, &config);
        late.hotplate_on = Some(at(11, 25).date());
        let (_, checker) = step(&mut memory, &late).ai_request.unwrap();
        let problems = checker.check(&decision(&[], &[("hotplate", "11:30", "14:00", 1800.0), ("geyser", "15:00", "17:00", 2000.0)])).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("cooked today already")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("must be off by 16:00")), "{problems:?}");
        let greedy = checker.check(&decision(&[], &[("geyser", "11:30", "16:00", 2000.0)])).unwrap_err();
        assert!(greedy.iter().any(|p| p.contains("mustn't go below") || p.contains("wouldn't be full")), "{greedy:?}");
    }

    #[test]
    fn a_checked_decision_runs_the_card_and_fills_in_what_the_geyser_already_gave() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &ai_step(at(11, 25), 1370.0, &sun, &config)).ai_request.unwrap();
        let answer = decision(&[("geyser", 1200.0, "10:57")], &[("geyser", "10:57", "13:30", 1200.0)]);
        let verified = checker.check(&answer).unwrap();
        memory.apply_ai(verified, &answer, Some(1370.0), at(11, 25));
        let shown = step(&mut memory, &ai_step(at(11, 26), 1370.0, &sun, &config));
        assert!(shown.ai_request.is_none());
        assert_eq!(shown.status.ai.mode, AiMode::Ai);
        assert_eq!(shown.status.geyser.row.phase, LoadPhase::Running);
        assert_eq!(shown.status.geyser.row.running_w, Some(1200.0));
        assert_eq!(shown.status.geyser.row.note.as_deref(), Some("Geyser on at 1200 W — the sun carries it until 2 PM."));
        assert!(shown.status.geyser.delivered_kwh >= 0.5, "{}", shown.status.geyser.delivered_kwh);
    }

    #[test]
    fn a_question_is_sent_once_and_a_no_drops_the_guess_and_asks_again() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &ai_step(at(11, 25), 1370.0, &sun, &config)).ai_request.unwrap();
        let mut unsure = decision(&[("geyser", 1200.0, "10:57")], &[("geyser", "10:57", "13:30", 1200.0)]);
        unsure.sure = false;
        unsure.question = "Is the geyser on at 1200 W?".into();
        memory.apply_ai(checker.check(&unsure).unwrap(), &unsure, Some(1370.0), at(11, 25));
        let asked = step(&mut memory, &ai_step(at(11, 26), 1370.0, &sun, &config));
        assert_eq!(asked.status.question.as_ref().map(|q| q.text.as_str()), Some("Is the geyser on at 1200 W?"));
        assert!(asked.asks.iter().any(|ask| ask.title == LOADS_TITLE && ask.body.starts_with("Is the geyser on")));
        assert!(step(&mut memory, &ai_step(at(11, 27), 1370.0, &sun, &config)).asks.is_empty());
        let mut no = ai_step(at(11, 28), 1370.0, &sun, &config);
        no.answer = Some(LoadsAnswer::No);
        let after = step(&mut memory, &no);
        assert!(after.status.question.is_none());
        assert!(memory.detector.running(LoadKind::Geyser).is_none());
        let (input, _) = after.ai_request.expect("asked again after the answer");
        assert!(input.answers[0].contains("the family said no"));
    }

    #[test]
    fn one_guess_of_the_hotplate_doesnt_make_it_a_cooking_day() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &ai_step(at(11, 39), 1363.0, &sun, &config)).ai_request.unwrap();
        let guess = decision(&[("hotplate", 1200.0, "10:34")], &[("hotplate", "11:39", "1:00 PM", 1200.0)]);
        memory.apply_ai(checker.check(&guess).unwrap(), &guess, Some(1363.0), at(11, 39));
        assert!(!step(&mut memory, &ai_step(at(11, 41), 1363.0, &sun, &config)).hotplate_cooked);
        let geyser = decision(&[("geyser", 1200.0, "10:57")], &[("geyser", "11:44", "14:00", 1200.0)]);
        memory.apply_ai(checker.check(&geyser).unwrap(), &geyser, Some(1363.0), at(11, 44));
        let after = step(&mut memory, &ai_step(at(11, 46), 1363.0, &sun, &config));
        assert!(!after.hotplate_cooked);
        assert_ne!(after.status.hotplate.row.phase, LoadPhase::Done);

        memory.apply_ai(checker.check(&guess).unwrap(), &guess, Some(1363.0), at(12, 0));
        memory.apply_ai(checker.check(&guess).unwrap(), &guess, Some(1363.0), at(12, 15));
        assert!(step(&mut memory, &ai_step(at(12, 31), 1363.0, &sun, &config)).hotplate_cooked);
    }

    #[test]
    fn an_edit_overrides_what_the_app_guessed_and_binds_the_ai() {
        let config = cooking();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 1500.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 1500.0, 1500.0, Some(&sun), &config));
        memory.geyser_heated_at = Some(at(13, 12));
        memory.geyser_delivered_wh = 2700.0;

        let edits = [
            LoadEdit { load: LoadKind::Geyser, state: EditState::Off, watts: None, kwh: Some(1.4), soc: None },
            LoadEdit { load: LoadKind::Hotplate, state: EditState::On, watts: Some(1500.0), kwh: None, soc: None },
        ];
        let mut edited = ai_step(at(13, 20), 1700.0, &sun, &config);
        edited.edits = &edits;
        edited.hotplate_on = Some(at(13, 20).date());
        let result = step(&mut memory, &edited);
        assert!(memory.geyser_heated_at.is_none());
        assert_eq!(result.status.geyser.delivered_kwh, 1.4);
        assert_ne!(result.status.geyser.row.phase, LoadPhase::Done);
        assert_eq!(result.status.hotplate.row.running_w, Some(1500.0));

        let (input, checker) = result.ai_request.expect("asked after the edit");
        assert!(input.answers.iter().any(|said| said.contains("geyser to off: switched off by hand")));
        let contradicts = decision(&[("geyser", 1200.0, "10:57"), ("hotplate", 200.0, "13:20")], &[]);
        let problems = checker.check(&contradicts).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("says the geyser is off")), "{problems:?}");
        assert!(checker.check(&decision(&[("hotplate", 1500.0, "13:20")], &[("hotplate", "13:20", "14:30", 1500.0)])).is_ok());
    }

    #[test]
    fn two_live_reads_tell_whether_the_scooter_is_charging() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let read = |soc: f64, hour: u32, minute: u32| EvReading { soc, at: at(hour, minute), live: true };
        let tick = |memory: &mut LoadsMemory, reading: EvReading, now: NaiveDateTime| {
            let mut next = ai_step(now, 1600.0, &sun, &config);
            next.ev_reading = Some(reading);
            step(memory, &next)
        };
        tick(&mut memory, read(68.0, 13, 20), at(13, 20));
        assert!(memory.detector.running(LoadKind::Ev).is_none());
        let rose = tick(&mut memory, read(72.0, 13, 35), at(13, 35));
        let charger = memory.detector.running(LoadKind::Ev).expect("charging");
        assert_eq!(charger.watts, config.ev_charger_watts);
        assert!(charger.since >= at(13, 20) && charger.since < at(13, 35));
        assert!(rose.status.ev.charge_check.as_deref().unwrap().starts_with("Charging: 68.0% at 13:20"));
        let (input, checker) = rose.ai_request.expect("the AI hears about it");
        assert!(input.answers.iter().any(|said| said.contains("so it is charging")));
        assert!(checker.check(&decision(&[], &[])).unwrap_err().iter().any(|p| p.contains("says the scooter is on")));

        let same = tick(&mut memory, read(72.0, 13, 50), at(13, 50));
        assert!(memory.detector.running(LoadKind::Ev).is_none());
        assert!(same.status.ev.charge_check.as_deref().unwrap().starts_with("Not charging"));
    }

    #[test]
    fn a_live_read_above_the_last_known_percent_means_charging() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut first = ai_step(at(13, 0), 1600.0, &sun, &config);
        first.ev_reading = Some(EvReading { soc: 72.0, at: at(13, 0), live: false });
        step(&mut memory, &first);
        let mut live = ai_step(at(14, 3), 1600.0, &sun, &config);
        live.ev_reading = Some(EvReading { soc: 81.0, at: at(14, 3), live: true });
        let result = step(&mut memory, &live);
        assert!(memory.detector.running(LoadKind::Ev).is_some());
        assert!(result.status.ev.charge_check.as_deref().unwrap().starts_with("Charging: 72.0% last known"));
    }

    #[test]
    fn the_base_house_load_leaves_out_loads_that_run_part_of_the_hour() {
        let day = at(12, 0).date();
        let samples: Vec<Sample> = (1..=7)
            .flat_map(|back| {
                (0..12).map(move |minute| Sample {
                    at: (day - Duration::days(back)).and_hms_opt(12, minute * 5, 0).unwrap(),
                    soc: Some(100.0),
                    load_w: Some(if minute < 8 { 1350.0 } else { 150.0 }),
                    pv_w: Some(1400.0),
                    battery_a: Some(0.0),
                    battery_v: Some(26.0),
                    grid_on: Some(true),
                    grid_basis: String::new(),
                    mode: Some("SBG".into()),
                    source: String::new(),
                })
            })
            .collect();
        let base = crate::automation::history::day_base_profile(&samples, day, 7);
        assert_eq!(base.iter().find(|h| h.hour == 12).map(|h| h.load_w), Some(150.0));
    }

    fn with_tomorrow(mut forecast: Forecast, radiation_kwh_m2: f64) -> Forecast {
        let tomorrow = at(12, 0).date() + Duration::days(1);
        forecast.daily.push(DayWeather { date: tomorrow, radiation_kwh_m2: Some(radiation_kwh_m2), sunshine_h: None, rain_prob_pct: None, temp_min_c: None });
        forecast
    }

    fn scooter_at<'a>(now: NaiveDateTime, soc: f64, sun: &'a Forecast, config: &'a AutomationConfig) -> LoadsInput<'a> {
        LoadsInput { ev_reading: Some(EvReading { soc, at: at(9, 0), live: false }), ..input(now, 100.0, 1500.0, 300.0, Some(sun), config) }
    }

    #[test]
    fn the_scooter_is_read_live_once_a_day_from_the_check_time() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        assert_ne!(step(&mut memory, &input(at(10, 29), 100.0, 1500.0, 300.0, Some(&sun), &config)).ev_read, Some(true));
        assert_eq!(step(&mut memory, &input(at(10, 30), 100.0, 1500.0, 300.0, Some(&sun), &config)).ev_read, Some(true));
        assert_ne!(step(&mut memory, &input(at(10, 45), 100.0, 1500.0, 300.0, Some(&sun), &config)).ev_read, Some(true), "30 minutes between tries");
        let read = step(&mut memory, &LoadsInput {
            ev_reading: Some(EvReading { soc: 96.0, at: at(10, 31), live: true }),
            ..input(at(11, 0), 100.0, 1500.0, 300.0, Some(&sun), &config)
        });
        assert!(read.ev_checked);
        assert_eq!(read.status.ev.checked_at, Some(at(10, 31)));
        assert_ne!(step(&mut memory, &input(at(11, 31), 100.0, 1500.0, 300.0, Some(&sun), &config)).ev_read, Some(true));

        let mut saved = LoadsMemory::default();
        let mut checked = input(at(11, 0), 100.0, 1500.0, 300.0, Some(&sun), &config);
        checked.ev_checked_on = Some(at(11, 0).date());
        assert_ne!(step(&mut saved, &checked).ev_read, Some(true), "already done today, before a restart");
    }

    #[test]
    fn an_unreachable_scooter_is_tried_three_times() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let tries = [(10, 30), (11, 0), (11, 30), (12, 0), (12, 30)]
            .iter()
            .filter(|(h, m)| step(&mut memory, &input(at(*h, *m), 100.0, 1500.0, 300.0, Some(&sun), &config)).ev_read == Some(true))
            .count();
        assert_eq!(tries, 3);
        assert!(step(&mut memory, &input(at(12, 40), 100.0, 1500.0, 300.0, Some(&sun), &config)).status.ev.check_failed);
    }

    #[test]
    fn at_or_below_the_threshold_the_scooter_charges_today() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &scooter_at(at(10, 0), 72.0, &sun, &config));
        assert_eq!(result.status.ev.charge_today, Some(true));
        assert!(result.status.plan.iter().any(|slot| slot.kind == LoadKind::Ev));
    }

    #[test]
    fn above_the_threshold_with_a_good_tomorrow_no_charge_is_needed() {
        let config = config();
        let sun = with_tomorrow(sunny(800.0), 3.6);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &scooter_at(at(10, 0), 96.0, &sun, &config));
        assert_eq!(result.status.ev.charge_today, Some(false));
        assert_eq!(result.status.ev.row.phase, LoadPhase::Done);
        assert!(result.status.ev.row.note.as_deref().unwrap().starts_with("No charge needed"));
        assert!(result.status.plan.iter().all(|slot| slot.kind != LoadKind::Ev));
    }

    #[test]
    fn above_the_threshold_it_charges_when_tomorrow_is_cloudy_and_today_sunny() {
        let config = config();
        let sun = with_tomorrow(sunny(800.0), 0.4);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &scooter_at(at(10, 0), 90.0, &sun, &config));
        assert_eq!(result.status.ev.charge_today, Some(true));
        assert!(result.status.ev.charge_rule.as_deref().unwrap().contains("tomorrow looks too cloudy"));
        assert!(result.status.plan.iter().any(|slot| slot.kind == LoadKind::Ev));
    }

    #[test]
    fn plug_in_reminders_repeat_every_half_hour_until_the_charger_shows_up() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let reminded = |result: &LoadsStep| result.notify.iter().any(|(title, body)| *title == EV_TITLE && body.starts_with("Plug in"));
        assert!(reminded(&step(&mut memory, &scooter_at(at(11, 0), 72.0, &sun, &config))));
        assert!(!reminded(&step(&mut memory, &input(at(11, 15), 100.0, 1500.0, 300.0, Some(&sun), &config))));
        assert!(reminded(&step(&mut memory, &input(at(11, 30), 100.0, 1500.0, 300.0, Some(&sun), &config))));
        let plugged = step(&mut memory, &input(at(11, 32), 100.0, 1500.0, 900.0, Some(&sun), &config));
        assert_eq!(plugged.status.ev.row.phase, LoadPhase::Running);
        assert!(!reminded(&step(&mut memory, &input(at(12, 2), 100.0, 1500.0, 900.0, Some(&sun), &config))));
    }

    #[test]
    fn the_checker_holds_the_ai_to_the_scooter_rule() {
        let config = config();
        let fine = with_tomorrow(sunny(800.0), 3.6);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &LoadsInput { ai_available: true, ..scooter_at(at(10, 0), 96.0, &fine, &config) }).ai_request.unwrap();
        let problems = checker.check(&decision(&[], &[("scooter", "10:00", "10:30", 600.0)])).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("no charge needed today")), "{problems:?}");

        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &LoadsInput { ai_available: true, ..scooter_at(at(10, 0), 72.0, &sun, &config) }).ai_request.unwrap();
        let problems = checker.check(&decision(&[], &[])).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("must charge today")), "{problems:?}");
    }

    #[test]
    fn the_checker_uses_the_sky_the_readings_show_not_the_forecast() {
        let config = config();
        let sun = sunny(800.0);
        let geyser = decision(&[], &[("geyser", "10:00", "12:00", 1200.0)]);
        let mut memory = LoadsMemory::default();
        let (_, checker) = step(&mut memory, &LoadsInput { ai_available: true, ..input(at(9, 5), 46.0, 450.0, 150.0, Some(&sun), &config) })
            .ai_request
            .unwrap();
        assert!(checker.check(&geyser).is_ok(), "{:?}", checker.check(&geyser));

        let dark: Vec<PvPoint> = (0..96)
            .map(|step| {
                let at = at(0, 0) + Duration::minutes(15 * step);
                PvPoint { at, watts: if (7..17).contains(&at.hour()) { 450.0 } else { 0.0 } }
            })
            .collect();
        let mut memory = LoadsMemory::default();
        let (request, checker) = step(
            &mut memory,
            &LoadsInput { ai_available: true, pv_curve: &dark, sky_factor: Some(0.3), ..input(at(9, 5), 46.0, 450.0, 150.0, Some(&sun), &config) },
        )
        .ai_request
        .unwrap();
        assert!(request.hours.iter().all(|hour| hour.sun_w <= 450.0), "{:?}", request.hours);
        assert_eq!(request.sky_vs_forecast_pct, Some(30.0));
        let problems = checker.check(&geyser).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("only")), "{problems:?}");
    }

    #[test]
    fn a_motor_already_on_is_not_held_against_the_ai_on_a_dark_day() {
        let config = config();
        let sun = sunny(800.0);
        let dark: Vec<PvPoint> = (0..96)
            .map(|step| {
                let at = at(0, 0) + Duration::minutes(15 * step);
                PvPoint { at, watts: if (7..17).contains(&at.hour()) { 450.0 } else { 0.0 } }
            })
            .collect();
        let mut memory = LoadsMemory::default();
        let tick = |memory: &mut LoadsMemory, now: NaiveDateTime, load: f64, ai: bool| {
            step(
                memory,
                &LoadsInput { water_pending: true, ai_available: ai, pv_curve: &dark, ..input(now, 50.0, 450.0, load, Some(&sun), &config) },
            )
        };
        tick(&mut memory, at(11, 58), 200.0, false);
        tick(&mut memory, at(12, 0), 1200.0, false);
        let (_, checker) = tick(&mut memory, at(12, 2), 1200.0, true).ai_request.unwrap();
        let motor = decision(&[("water", 1000.0, "12:00")], &[("water", "12:00", "12:30", 1000.0)]);
        assert!(checker.check(&motor).is_ok(), "{:?}", checker.check(&motor));
        let more = decision(&[("water", 1000.0, "12:00")], &[("water", "12:00", "12:30", 1000.0), ("geyser", "12:30", "14:30", 800.0)]);
        let problems = checker.check(&more).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("only")), "{problems:?}");
    }

    #[test]
    fn a_nearly_full_scooter_is_not_charged_for_a_cloudy_tomorrow() {
        let config = config();
        let sun = with_tomorrow(sunny(800.0), 0.4);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &scooter_at(at(10, 0), 99.0, &sun, &config));
        assert_eq!(result.status.ev.charge_today, Some(false));
        assert!(result.status.plan.iter().all(|slot| slot.kind != LoadKind::Ev));
    }

    #[test]
    fn a_motor_run_that_stops_asks_whether_the_tank_is_full() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let tick = |memory: &mut LoadsMemory, now: NaiveDateTime, load: f64| {
            step(memory, &LoadsInput { water_pending: true, ..input(now, 100.0, 1500.0, load, Some(&sun), &config) })
        };
        tick(&mut memory, at(11, 0), 300.0);
        tick(&mut memory, at(11, 2), 1300.0);
        assert!(tick(&mut memory, at(11, 15), 1300.0).status.tank_question.is_none());
        let stopped = tick(&mut memory, at(11, 22), 300.0);
        assert_eq!(stopped.status.tank_question, Some(at(11, 22)));
        let edits = [LoadEdit { load: LoadKind::Water, state: EditState::Done, watts: None, kwh: None, soc: None }];
        let answered = step(&mut memory, &LoadsInput { edits: &edits, water_pending: false, ..input(at(11, 23), 100.0, 1500.0, 300.0, Some(&sun), &config) });
        assert!(answered.status.tank_question.is_none());
    }

    #[test]
    fn the_spare_sun_hours_start_now_and_carry_a_running_load_through_its_run() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(10, 50), 100.0, 1500.0, 300.0, Some(&sun), &config));
        let result = step(&mut memory, &input(at(10, 52), 100.0, 1500.0, 1500.0, Some(&sun), &config));
        let hours = &result.status.sun_hours;
        assert_eq!(hours[0].at, at(10, 52));
        assert_eq!(hours[1].at, at(11, 0));
        assert_eq!(hours[0].loads_w, 1200.0);
        let geyser = result.status.plan.iter().find(|slot| slot.kind == LoadKind::Geyser).unwrap();
        let after = hours.iter().find(|hour| hour.at >= geyser.end).unwrap();
        assert_eq!(after.loads_w, 0.0);
        assert!(hours.last().unwrap().at < at(17, 15));
    }

    #[test]
    fn a_rejected_decision_falls_back_to_the_apps_plan() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &ai_step(at(11, 25), 300.0, &sun, &config));
        memory.ai_rejected(at(11, 25), vec!["bad".into()], "x".into(), None);
        let shown = step(&mut memory, &ai_step(at(11, 26), 300.0, &sun, &config));
        assert_eq!(shown.status.ai.mode, AiMode::Fallback);
        assert_eq!(shown.status.ai.problems, vec!["bad".to_string()]);
        assert!(shown.status.plan.iter().any(|slot| slot.kind == LoadKind::Geyser));
    }

    #[test]
    fn a_full_battery_lets_the_scooter_start_now_beside_the_tank() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut now = input(at(10, 15), 100.0, 150.0, 160.0, Some(&sun), &config);
        now.water_pending = true;
        now.ev_reading = Some(EvReading { soc: 63.0, at: at(10, 10), live: false });
        let result = step(&mut memory, &now);
        let water = result.status.plan.iter().find(|s| s.kind == LoadKind::Water).unwrap();
        let ev = result.status.plan.iter().find(|s| s.kind == LoadKind::Ev).unwrap();
        assert_eq!(ev.start, at(10, 15));
        assert!(ev.end > water.start);
        assert_eq!(result.status.ev.row.phase, LoadPhase::Ready);
    }

    #[test]
    fn the_loads_draw_the_battery_no_lower_than_the_day_floor() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let result = step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        let pv_at = |at: NaiveDateTime| sun.radiation_during(at).map(|r| r * 4.0 * 0.75);
        let house = |_: u32| Some(300.0);
        let probe = Sun {
            now: at(11, 0),
            sunset: at(17, 15),
            soc: 100.0,
            rated_ah: 100.0,
            volts: 26.0,
            max_charge_a: Some(60.0),
            floor: config.min_soc_percent,
            day_low: config.day_low_soc,
            pv_now_w: Some(400.0_f64.max(pv_at(at(11, 0)).unwrap())),
            house_now_w: Some(300.0),
            pv_at: &pv_at,
            house_at: &house,
        };
        let steps = steps(&probe);
        let mut extra = vec![0.0; steps.len()];
        for slot in &result.status.plan {
            for (i, step) in steps.iter().enumerate() {
                if step.start >= slot.start && step.start < slot.end {
                    extra[i] += slot.watts;
                }
            }
        }
        assert!(simulate(&probe, &steps, &extra).low_soc >= config.day_low_soc - 0.5);
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
    fn an_appliance_going_off_isnt_taken_as_the_scooter() {
        let config = config();
        let options = load_options(&config);
        let mut detector = Detector::default();
        detector.observe(at(8, 15), 0.0, &options, &[]);
        assert_eq!(detector.observe(at(8, 17), 605.0, &options, &[]), vec![Change::Started(LoadKind::Ev, 600.0)]);
        assert!(detector.observe(at(8, 21), 1139.0, &options, &[]).is_empty());
        assert!(detector.observe(at(8, 23), 595.0, &options, &[]).is_empty());
        assert!(detector.running(LoadKind::Ev).is_some());
        assert_eq!(detector.observe(at(8, 30), 60.0, &options, &[]), vec![Change::Stopped(LoadKind::Ev, 600.0)]);
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
    fn a_geyser_found_lower_after_a_missed_reading_steps_down_instead_of_vanishing() {
        let config = config();
        let options = load_options(&config);
        let mut detector = Detector::default();
        detector.observe(at(10, 0), 250.0, &options, &[]);
        detector.observe(at(10, 2), 2250.0, &options, &[]);
        detector.last = None;
        let changes = detector.observe(at(11, 3), 1104.0, &options, &[]);
        assert_eq!(changes, vec![Change::Level(LoadKind::Geyser, 2000.0, 800.0)]);
        assert_eq!(detector.running(LoadKind::Geyser).map(|r| r.watts), Some(800.0));
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
    fn an_on_press_that_switched_off_a_hidden_element_presses_it_again() {
        let config = config();
        let mut memory = LoadsMemory::default();
        let mut notify = Vec::new();
        let on = Verdict { phase: LoadPhase::Ready, note: String::new(), watts: Some(800.0), slot: None };
        memory.geyser.last_load_w = Some(2200.0);
        let command = geyser_command(&mut memory, &on, None, Control::Full, at(11, 54), &config, &mut notify).unwrap();
        assert_eq!(command.buttons, vec![Button::W800]);
        memory.geyser_sent(&command, at(11, 54), true);
        memory.geyser.last_load_w = Some(1489.0);
        memory.geyser.reading_at = Some(at(11, 58));
        let again = geyser_command(&mut memory, &on, None, Control::Full, at(11, 58), &config, &mut notify).unwrap();
        assert_eq!(again.buttons, vec![Button::W800]);
        assert!(memory.geyser.failed_on.is_none());
        assert!(notify.iter().any(|(_, text)| text.contains("already heating")));
    }

    #[test]
    fn the_thermostat_cutting_out_marks_the_water_hot() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        for minute in (10..=50).step_by(10) {
            step(&mut memory, &input(at(11, minute), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        }
        let running = step(&mut memory, &input(at(12, 0), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        assert_eq!(running.status.geyser.row.phase, LoadPhase::Running);
        let cut = step(&mut memory, &input(at(12, 2), 100.0, 300.0, 300.0, Some(&sun), &config));
        assert_eq!(cut.status.geyser.row.phase, LoadPhase::Done);
        assert!(cut.notify.iter().any(|(_, body)| body.contains("the water is hot")));
        assert!(cut.status.geyser.delivered_kwh >= 0.75);
    }

    #[test]
    fn a_short_run_that_stops_is_not_taken_as_hot_water() {
        let config = config();
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        step(&mut memory, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config));
        step(&mut memory, &input(at(11, 2), 100.0, 1600.0, 1500.0, Some(&sun), &config));
        let off = step(&mut memory, &input(at(11, 4), 100.0, 300.0, 300.0, Some(&sun), &config));
        assert_ne!(off.status.geyser.row.phase, LoadPhase::Done);
        assert!(memory.geyser_heated_at.is_none());
        assert!(off.notify.iter().any(|(_, body)| body.contains("switched off by hand")));
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
    fn pv_ratio_learns_from_charging_samples() {
        let sun = sunny(800.0);
        let sample = |i: u32, amps: f64| Sample {
            at: at(12, i * 2),
            soc: Some(70.0),
            load_w: Some(300.0),
            pv_w: Some(800.0 * 4.0 * 0.62),
            battery_a: Some(amps),
            battery_v: Some(26.0),
            grid_on: Some(true),
            grid_basis: String::new(),
            mode: Some("SBG".into()),
            source: String::new(),
        };
        let mut samples: Vec<Sample> = (0..20).map(|i| sample(i, if i % 2 == 0 { 20.0 } else { 47.0 })).collect();
        let ratio = pv_ratio(&samples, &sun, 4000.0, at(0, 0)).unwrap();
        let radiation = sun.radiation_during(at(12, 0)).unwrap();
        assert!((ratio - 800.0 * 4.0 * 0.62 / (radiation * 4.0)).abs() < 0.01);
        let undersized = pv_ratio(&samples, &sun, 2000.0, at(0, 0)).unwrap();
        assert!(undersized > 1.0, "a panel size set too low is learned past 1: {undersized}");
        assert_eq!(pv_ratio(&samples[..5], &sun, 4000.0, at(0, 0)), None);
        samples.iter_mut().for_each(|s| s.battery_a = Some(0.2));
        assert_eq!(pv_ratio(&samples, &sun, 4000.0, at(0, 0)), None);
    }

    #[test]
    fn readings_at_the_charge_limit_or_full_only_set_a_floor() {
        let sun = sunny(800.0);
        let radiation = sun.radiation_during(at(12, 0)).unwrap();
        let sample = |i: u32, pv: f64, amps: f64, soc: f64| Sample {
            at: at(12, i),
            soc: Some(soc),
            load_w: Some(300.0),
            pv_w: Some(pv),
            battery_a: Some(amps),
            battery_v: Some(27.0),
            grid_on: Some(false),
            grid_basis: String::new(),
            mode: Some("SBG".into()),
            source: String::new(),
        };
        let capped: Vec<Sample> = (0..14).map(|i| sample(i, radiation * 4.0 * 0.4, 46.6, 70.0)).collect();
        let free: Vec<Sample> = (20..34).map(|i| sample(i, radiation * 4.0 * 0.5, 20.0, 70.0)).collect();
        let full: Vec<Sample> = (40..54).map(|i| sample(i, radiation * 4.0 * 0.7, 0.0, 100.0)).collect();
        let mixed: Vec<Sample> = capped.iter().chain(&free).cloned().collect();
        let ratio = pv_ratio(&mixed, &sun, 4000.0, at(0, 0)).unwrap();
        assert!((ratio - 0.5).abs() < 0.01, "capped readings don't pull it down: {ratio}");
        let with_full: Vec<Sample> = mixed.iter().chain(&full).cloned().collect();
        let ratio = pv_ratio(&with_full, &sun, 4000.0, at(0, 0)).unwrap();
        assert!((ratio - 0.7).abs() < 0.01, "a full battery's readings set a floor above it: {ratio}");
    }

    #[test]
    fn geyser_readings_map_to_its_elements() {
        let config = config();
        assert_eq!(geyser_elements(800.0, &config), Some((true, false)));
        assert_eq!(geyser_elements(1150.0, &config), Some((false, true)));
        assert_eq!(geyser_elements(2000.0, &config), Some((true, true)));
        assert_eq!(geyser_elements(400.0, &config), None);
    }

    #[test]
    fn element_toggles_add_before_they_remove() {
        let config = config();
        assert_eq!(geyser_toggles((true, false), 1200.0, &config), Some(vec![Button::W1200, Button::W800]));
        assert_eq!(geyser_toggles((false, true), 800.0, &config), Some(vec![Button::W800, Button::W1200]));
        assert_eq!(geyser_toggles((true, true), 1200.0, &config), Some(vec![Button::W800]));
        assert_eq!(geyser_toggles((true, true), 800.0, &config), Some(vec![Button::W1200]));
        assert_eq!(geyser_toggles((false, true), 2000.0, &config), Some(vec![Button::W800]));
        assert_eq!(geyser_toggles((true, false), 800.0, &config), None);
    }

    fn running_app() -> LoadsMemory {
        let mut memory = LoadsMemory::default();
        memory.geyser.readings_seen = READINGS_BEFORE_STEP_UP;
        memory
    }

    #[test]
    fn after_a_start_nothing_is_pressed_until_a_fresh_reading_and_beliefs_come_back() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut started = LoadsMemory::default();
        assert!(step(&mut started, &auto_input(at(11, 0), 300.0, &sun, &config)).geyser_command.is_none(), "the first reading may be old");
        assert!(step(&mut started, &auto_input(at(11, 1), 300.0, &sun, &config)).geyser_command.is_none(), "the same reading again");
        assert!(step(&mut started, &auto_input(at(11, 2), 310.0, &sun, &config)).geyser_command.is_some());

        let by_hand = AutomationConfig { geyser_auto_control: false, ..config.clone() };
        let mut before = running_app();
        step(&mut before, &input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &by_hand));
        step(&mut before, &input(at(11, 2), 100.0, 1600.0, 1500.0, Some(&sun), &by_hand));
        let saved = before.saved(at(11, 3));
        let mut after = LoadsMemory::default();
        assert!(after.restore(saved.clone(), at(11, 10)));
        assert_eq!(after.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(1200.0));
        assert!(!LoadsMemory::default().restore(saved, at(11, 40)), "too old to trust");
    }

    fn auto_input<'a>(now: NaiveDateTime, load: f64, sun: &'a Forecast, config: &'a AutomationConfig) -> LoadsInput<'a> {
        let mut input = input(now, 100.0, 400.0, load, Some(sun), config);
        input.remote_ready = true;
        input
    }

    #[test]
    fn a_manual_switch_on_is_checked_watched_and_then_decided() {
        let config = AutomationConfig { geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let mut ask = input(at(11, 0), 100.0, 400.0, 300.0, Some(&sun), &config);
        ask.geyser_request = Some(GeyserIntent::On(1200.0));
        let first = step(&mut memory, &ask);
        let command = first.geyser_command.unwrap();
        assert_eq!(command, GeyserCommand { intent: GeyserIntent::On(1200.0), buttons: vec![Button::W1200] });
        assert!(first.notify.iter().any(|(_, body)| body.starts_with("You asked for the geyser on at 1200 W")));
        memory.geyser_sent(&command, at(11, 0), true);

        let on = step(&mut memory, &input(at(11, 2), 100.0, 400.0, 1500.0, Some(&sun), &config));
        assert!(on.geyser_command.is_none());
        assert_eq!(on.status.geyser.check.as_ref().map(|check| check.state), Some(CheckState::Confirmed));
        let until = on.status.geyser.watch_until.expect("watched after it comes on");
        assert!(on.status.geyser.row.note.as_deref().is_some_and(|note| note.contains("watching")));

        let during = step(&mut memory, &input(until - Duration::minutes(1), 100.0, 400.0, 1500.0, Some(&sun), &config));
        assert!(!during.notify.iter().any(|(_, body)| body.starts_with("Watched")));
        let after = step(&mut memory, &input(until, 100.0, 400.0, 1500.0, Some(&sun), &config));
        assert_eq!(after.status.geyser.watch_until, None);
        let decided = after.notify.iter().find(|(_, body)| body.starts_with("Watched 10 min at 1200 W")).expect("the watch is summed up");
        assert!(decided.1.contains("Suggestion:"), "{}", decided.1);
        let later = step(&mut memory, &input(until + Duration::minutes(1), 100.0, 400.0, 1500.0, Some(&sun), &config));
        assert!(!later.notify.iter().any(|(_, body)| body.starts_with("Watched")), "told once");
    }

    #[test]
    fn the_watch_readings_say_whether_the_battery_drains() {
        let sample = |minute: u32, battery_a: f64| Sample {
            at: at(12, minute),
            soc: Some(99.0 - f64::from(minute) / 10.0),
            load_w: Some(1700.0),
            pv_w: Some(1400.0),
            battery_a: Some(battery_a),
            battery_v: Some(52.0),
            grid_on: Some(true),
            grid_basis: String::new(),
            mode: Some("SBG".into()),
            source: String::new(),
        };
        let samples: Vec<Sample> = (0..10).map(|minute| sample(minute, -6.0)).collect();
        let summary = watch_summary(&samples, at(12, 0)).unwrap();
        assert_eq!(summary, "sun 1.4 kW, house 1.7 kW with the geyser, battery draining 312 W, 99% → 98%");
        assert!(watch_summary(&samples, at(13, 0)).is_none());
    }

    #[test]
    fn auto_control_presses_only_the_elements_and_checks_the_load() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let first = step(&mut memory, &auto_input(at(11, 0), 300.0, &sun, &config));
        let command = first.geyser_command.unwrap();
        let GeyserIntent::On(target) = command.intent else { panic!("expected an On command") };
        assert_eq!(Some(command.buttons.clone()), geyser_toggles((false, false), target, &config));
        assert!(!command.buttons.contains(&Button::Power), "On/Off is never pressed");
        memory.geyser_sent(&command, at(11, 0), true);
        assert_eq!(memory.geyser.check.as_ref().map(|check| check.state), Some(CheckState::Checking));

        let settled = step(&mut memory, &auto_input(at(11, 2), 300.0 + target, &sun, &config));
        assert!(settled.geyser_command.is_none());
        assert!(settled.notify.iter().any(|(_, body)| body.starts_with("Confirmed: the geyser is on")));
        assert_eq!(settled.status.geyser.check.map(|check| check.state), Some(CheckState::Confirmed));
    }

    #[test]
    fn a_level_that_comes_out_wrong_is_fixed_from_what_the_load_shows() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let command = step(&mut memory, &auto_input(at(11, 0), 300.0, &sun, &config)).geyser_command.unwrap();
        let GeyserIntent::On(target) = command.intent else { panic!("expected an On command") };
        memory.geyser_sent(&command, at(11, 0), true);
        let other = if (target - config.geyser_low_watts).abs() > 1.0 { config.geyser_low_watts } else { config.geyser_high_watts };
        let other_elements = geyser_elements(other, &config).unwrap();
        assert!(step(&mut memory, &auto_input(at(11, 2), 300.0 + other, &sun, &config)).geyser_command.is_none(), "waits a few minutes");
        let fix = step(&mut memory, &auto_input(at(11, 4), 310.0 + other, &sun, &config)).geyser_command.unwrap();
        assert_eq!(Some(fix.buttons), geyser_toggles(other_elements, target, &config));
    }

    #[test]
    fn a_switch_on_that_draws_nothing_is_not_repeated() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let command = step(&mut memory, &auto_input(at(11, 0), 300.0, &sun, &config)).geyser_command.unwrap();
        memory.geyser_sent(&command, at(11, 0), true);
        let stale = step(&mut memory, &auto_input(at(11, 4), 300.0, &sun, &config));
        assert!(stale.geyser_command.is_none() && memory.geyser.pending.is_some(), "no reading since the press yet");
        let quiet = step(&mut memory, &auto_input(at(11, 5), 310.0, &sun, &config));
        assert!(quiet.geyser_command.is_none());
        assert!(quiet.notify.iter().any(|(_, body)| body.contains("isn't drawing power")));
        assert_eq!(memory.geyser.failed_on, Some(at(11, 5).date()));
        assert_eq!(quiet.status.geyser.check.map(|check| check.state), Some(CheckState::NotConfirmed));
        for minutes in [10, 30, 60] {
            let later = auto_input(at(11, 5) + Duration::minutes(minutes), 300.0, &sun, &config);
            assert!(step(&mut memory, &later).geyser_command.is_none(), "nothing is pressed again the same day");
        }
    }

    #[test]
    fn switching_off_toggles_the_elements_that_are_on() {
        let config = config();
        let off = Verdict { phase: LoadPhase::Stop, note: "Sunset.".into(), watts: None, slot: None };
        let mut notify = Vec::new();
        let mut memory = LoadsMemory::default();
        assert!(geyser_command(&mut memory, &off, None, Control::Full, at(17, 30), &config, &mut notify).is_none());

        let both = Running { watts: 2000.0, since: at(17, 0) };
        let command = geyser_command(&mut memory, &off, Some(&both), Control::Full, at(17, 30), &config, &mut notify).unwrap();
        assert_eq!(command, GeyserCommand { intent: GeyserIntent::Off, buttons: vec![Button::W800, Button::W1200] });
        memory.geyser_sent(&command, at(17, 30), true);
        memory.geyser.reading_at = Some(at(17, 34));
        let high = Running { watts: 1200.0, since: at(17, 0) };
        let again = geyser_command(&mut memory, &off, Some(&high), Control::Full, at(17, 34), &config, &mut notify).unwrap();
        assert_eq!(again.buttons, vec![Button::W1200], "only what is still on");
        memory.geyser_sent(&again, at(17, 34), true);
        assert!(geyser_command(&mut memory, &off, None, Control::Full, at(17, 36), &config, &mut notify).is_none());
        assert_eq!(memory.geyser.check.as_ref().map(|check| check.state), Some(CheckState::Confirmed));
        assert!(notify.iter().all(|(_, body)| !body.contains("On/Off")));
    }

    #[test]
    fn after_a_failed_day_a_drawing_geyser_still_has_its_elements_switched_off() {
        let config = config();
        let off = Verdict { phase: LoadPhase::Stop, note: "Switch the geyser off for the night.".into(), watts: None, slot: None };
        let down = Verdict { phase: LoadPhase::Stop, note: "Clouds.".into(), watts: Some(config.geyser_low_watts), slot: None };
        let on = Verdict { phase: LoadPhase::Ready, note: "Sun.".into(), watts: Some(config.geyser_low_watts), slot: None };
        let drawing = Running { watts: config.geyser_low_watts + config.geyser_high_watts, since: at(18, 0) };
        let mut notify = Vec::new();
        let mut memory = LoadsMemory::default();
        assert!(geyser_command(&mut memory, &on, None, Control::OffOnly, at(18, 0), &config, &mut notify).is_none());
        assert!(geyser_command(&mut memory, &down, Some(&drawing), Control::OffOnly, at(18, 0), &config, &mut notify).is_none());
        let command = geyser_command(&mut memory, &off, Some(&drawing), Control::OffOnly, at(18, 0), &config, &mut notify).unwrap();
        assert_eq!(command, GeyserCommand { intent: GeyserIntent::Off, buttons: vec![Button::W800, Button::W1200] });
    }

    #[test]
    fn manual_switching_works_from_the_load_and_never_presses_on_off() {
        let config = AutomationConfig { geyser_esp32_host: "geyser".into(), ..config() };
        let high = Running { watts: 1200.0, since: at(12, 0) };
        let mut notify = Vec::new();
        let mut memory = LoadsMemory::default();
        memory.geyser.room_w = 3000.0;
        assert!(manual_geyser(&mut memory, GeyserIntent::Off, None, true, &config, &mut notify).is_none());
        assert!(notify[0].1.contains("already off"));
        assert_eq!(manual_geyser(&mut memory, GeyserIntent::Off, Some(&high), true, &config, &mut notify).unwrap().buttons, vec![Button::W1200]);
        assert_eq!(manual_geyser(&mut memory, GeyserIntent::On(800.0), Some(&high), true, &config, &mut notify).unwrap().buttons, vec![Button::W800, Button::W1200]);
        assert_eq!(manual_geyser(&mut memory, GeyserIntent::On(2000.0), None, true, &config, &mut notify).unwrap().buttons, vec![Button::W800, Button::W1200]);
        assert!(manual_geyser(&mut memory, GeyserIntent::On(1200.0), Some(&high), true, &config, &mut notify).is_none());
        assert!(notify.last().unwrap().1.contains("already on at 1200 W"));

        memory.geyser.unknown_w = 1900.0;
        assert!(manual_geyser(&mut memory, GeyserIntent::On(2000.0), None, true, &config, &mut notify).is_none());
        assert!(notify.last().unwrap().1.contains("can't place"));
    }

    #[test]
    fn with_the_esp32_in_charge_a_load_step_is_the_geyser_only_after_a_press() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let quiet = |at_: NaiveDateTime, load: f64| {
            let mut input = auto_input(at_, load, &sun, &config);
            input.remote_ready = false;
            input
        };
        let mut unheard = running_app();
        step(&mut unheard, &quiet(at(11, 0), 300.0));
        step(&mut unheard, &quiet(at(11, 2), 1500.0));
        assert!(unheard.detector.running(LoadKind::Geyser).is_none(), "nobody pressed: another load");

        let mut heard = running_app();
        step(&mut heard, &quiet(at(11, 0), 300.0));
        let mut remote = quiet(at(11, 2), 1500.0);
        remote.geyser_heard_at = Some(at(11, 1));
        step(&mut heard, &remote);
        assert_eq!(heard.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(1200.0));
        step(&mut heard, &quiet(at(11, 30), 2500.0));
        assert_eq!(heard.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(1200.0), "a later step isn't a level change");
        step(&mut heard, &quiet(at(11, 32), 1300.0));
        step(&mut heard, &quiet(at(11, 34), 300.0));
        assert!(heard.detector.running(LoadKind::Geyser).is_none(), "its thermostat can still switch it off");
    }

    #[test]
    fn the_hotplate_coming_on_switches_the_geyser_off_at_once() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let mut ask = auto_input(at(11, 0), 300.0, &sun, &config);
        ask.geyser_request = Some(GeyserIntent::On(1200.0));
        let command = step(&mut memory, &ask).geyser_command.unwrap();
        memory.geyser_sent(&command, at(11, 0), true);
        step(&mut memory, &auto_input(at(11, 2), 1500.0, &sun, &config));
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(1200.0));

        let cooking = step(&mut memory, &auto_input(at(11, 4), 2600.0, &sun, &config));
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(1200.0), "the step is the hotplate, not the geyser");
        let down = cooking.geyser_command.expect("the geyser comes down at once, press gap or not");
        assert_eq!(down, GeyserCommand { intent: GeyserIntent::On(800.0), buttons: vec![Button::W800, Button::W1200] });
        assert!(cooking.notify.iter().any(|(_, body)| body.contains("inverter isn't overloaded")));
    }

    #[test]
    fn beside_the_hotplate_the_geyser_runs_at_800_and_goes_off_above_3300_w() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        step(&mut memory, &auto_input(at(12, 40), 1100.0, &sun, &config));
        let mut yes = auto_input(at(12, 41), 1100.0, &sun, &config);
        yes.answer = Some(LoadsAnswer::Yes);
        let on = step(&mut memory, &yes);
        let command = on.geyser_command.expect("the geyser takes the sun at 800 W beside the hotplate");
        assert_eq!(command.intent, GeyserIntent::On(800.0));
        assert!(on.notify.iter().any(|(_, body)| body.contains("Keep the hotplate at 1800 W or below")), "{:?}", on.notify);
        let log = on.log.as_ref().expect("an answer and a press are logged");
        assert_eq!(log["command"]["buttons"], serde_json::json!(["800"]));
        assert_eq!(log["answer"], "Yes");
        assert_eq!(log["hotplate_maybe_on"], true);
        memory.geyser_sent(&command, at(12, 41), true);
        step(&mut memory, &auto_input(at(12, 46), 1900.0, &sun, &config));
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(800.0));

        let high = step(&mut memory, &auto_input(at(12, 51), 3500.0, &sun, &config));
        let off = high.geyser_command.expect("off at once above 3300 W");
        assert_eq!(off.intent, GeyserIntent::Off);
    }

    #[test]
    fn the_geyser_never_takes_more_than_the_inverter_has_left() {
        let config = AutomationConfig { geyser_esp32_host: "geyser".into(), inverter_max_w: 3600.0, ..config() };
        let sun = sunny(800.0);
        let input = |load: f64| LoadsInput { load_w: Some(load), ..input(at(11, 0), 100.0, 3000.0, load, Some(&sun), &config) };
        assert!((geyser_room(&input(1240.0), &None, &None, &None, false, &config) - 2000.0).abs() < 1.0);
        assert_eq!(geyser_room(&input(300.0), &None, &None, &None, true, &config), 800.0, "only the lowest level beside the hotplate");
        assert_eq!(geyser_room(&input(2300.0), &None, &None, &None, true, &config), 0.0, "no room to start below 3000 W");
        let low = Some(Running { watts: 800.0, since: at(12, 0) });
        assert_eq!(geyser_room(&input(3250.0), &low, &None, &None, true, &config), 800.0, "stays on up to 3300 W");
        assert_eq!(geyser_room(&input(3400.0), &low, &None, &None, true, &config), 0.0, "off above 3300 W");
        let both = Running { watts: 2000.0, since: at(10, 0) };
        let down = keep_inverter_safe(&both, 1300.0, false, &config);
        assert_eq!((down.phase, down.watts), (LoadPhase::Stop, Some(1200.0)));

        let mut memory = LoadsMemory::default();
        memory.geyser.room_w = 1000.0;
        let mut notify = Vec::new();
        let capped = manual_geyser(&mut memory, GeyserIntent::On(2000.0), None, true, &config, &mut notify).unwrap();
        assert_eq!(capped.intent, GeyserIntent::On(800.0));
        memory.geyser.room_w = 0.0;
        assert!(manual_geyser(&mut memory, GeyserIntent::On(800.0), None, true, &config, &mut notify).is_none());
    }

    #[test]
    fn the_element_lights_follow_the_load_the_thermostat_and_the_presses() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut by_hand = auto_input(at(11, 0), 300.0, &sun, &config);
        by_hand.remote_ready = false;
        let first = step(&mut memory, &by_hand);
        assert!(first.status.geyser.elements.is_none());
        let mut heating = auto_input(at(11, 2), 2300.0, &sun, &config);
        heating.remote_ready = false;
        heating.geyser_heard_at = Some(at(11, 1));
        let on = step(&mut memory, &heating).status.geyser.elements.unwrap();
        assert_eq!((on.low, on.high, on.unsure), (ElementState::Heating, ElementState::Heating, false));
        let mut cut = auto_input(at(11, 40), 300.0, &sun, &config);
        cut.remote_ready = false;
        let warm = step(&mut memory, &cut).status.geyser.elements.unwrap();
        assert_eq!((warm.low, warm.high), (ElementState::KeepWarm, ElementState::KeepWarm));
        memory.geyser_sent(&GeyserCommand { intent: GeyserIntent::Off, buttons: vec![Button::W1200] }, at(11, 41), true);
        let mut after = auto_input(at(11, 42), 300.0, &sun, &config);
        after.remote_ready = false;
        let one = step(&mut memory, &after).status.geyser.elements.unwrap();
        assert_eq!((one.low, one.high), (ElementState::KeepWarm, ElementState::Off));
        let mut remote = auto_input(at(11, 50), 300.0, &sun, &config);
        remote.remote_ready = false;
        remote.geyser_heard_at = Some(at(11, 49));
        remote.geyser_heard_button = Some(Button::W800);
        let pressed = step(&mut memory, &remote).status.geyser.elements.unwrap();
        assert_eq!((pressed.low, pressed.high, pressed.unsure), (ElementState::Off, ElementState::Off, false));
        assert!(pressed.why.contains("800 W pressed on the remote"));
        let mut same = auto_input(at(11, 51), 300.0, &sun, &config);
        same.remote_ready = false;
        same.geyser_heard_at = Some(at(11, 49) + Duration::seconds(2));
        same.geyser_heard_button = Some(Button::W800);
        let still = step(&mut memory, &same).status.geyser.elements.unwrap();
        assert_eq!(still.low, ElementState::Off);
        let mut power = auto_input(at(11, 55), 300.0, &sun, &config);
        power.remote_ready = false;
        power.geyser_heard_at = Some(at(11, 54));
        power.geyser_heard_button = Some(Button::Power);
        assert!(step(&mut memory, &power).status.geyser.elements.unwrap().unsure);
    }

    #[test]
    fn hot_water_has_its_elements_switched_off_so_it_doesnt_heat_again() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = LoadsMemory::default();
        let mut by_hand = auto_input(at(11, 0), 300.0, &sun, &config);
        by_hand.remote_ready = false;
        step(&mut memory, &by_hand);
        for minute in (2..=58).step_by(8) {
            let mut heating = auto_input(at(11, minute), 1500.0, &sun, &config);
            heating.remote_ready = false;
            heating.geyser_heard_at = Some(at(11, 1));
            step(&mut memory, &heating);
        }
        let cut = step(&mut memory, &auto_input(at(12, 2), 300.0, &sun, &config));
        assert_eq!(cut.status.geyser.row.phase, LoadPhase::Done);
        let command = cut.geyser_command.expect("the selected element is switched off");
        assert_eq!(command, GeyserCommand { intent: GeyserIntent::Off, buttons: vec![Button::W1200] });
    }

    #[test]
    fn the_family_saying_the_geyser_is_on_sticks_although_the_app_pressed_nothing() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        step(&mut memory, &auto_input(at(13, 47), 600.0, &sun, &config));
        step(&mut memory, &auto_input(at(13, 49), 2650.0, &sun, &config));
        let edits = [LoadEdit { load: LoadKind::Geyser, state: EditState::On, watts: Some(2000.0), kwh: None, soc: None }];
        let mut edited = auto_input(at(13, 50), 2650.0, &sun, &config);
        edited.edits = &edits;
        step(&mut memory, &edited);
        let later = step(&mut memory, &auto_input(at(13, 53), 2650.0, &sun, &config));
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(2000.0));
        assert_ne!(later.status.geyser.row.phase, LoadPhase::Waiting);
    }

    #[test]
    fn a_drain_asks_the_ai_at_once_and_the_app_turns_the_geyser_down_if_it_must() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let mut ask = auto_input(at(11, 0), 300.0, &sun, &config);
        ask.geyser_request = Some(GeyserIntent::On(2000.0));
        let command = step(&mut memory, &ask).geyser_command.unwrap();
        memory.geyser_sent(&command, at(11, 0), true);
        let on = step(&mut memory, &auto_input(at(11, 2), 2300.0, &sun, &config));
        assert_eq!(on.status.geyser.row.running_w, Some(2000.0));
        assert!(on.status.geyser.watch_until.is_some());

        let draining = |minute: u32, memory: &mut LoadsMemory| {
            let mut input = LoadsInput { ai_available: true, ..auto_input(at(11, minute), 2300.0, &sun, &config) };
            input.battery_w = Some(-900.0);
            step(memory, &input)
        };
        let first = draining(3, &mut memory);
        assert!(first.geyser_command.is_none(), "the AI gets the first say");
        let (ai_input, _) = first.ai_request.expect("a drain asks the AI at once");
        assert!(ai_input.geyser.contains("draining 900 W"), "{}", ai_input.geyser);
        assert_eq!(first.status.geyser.watch_until, None, "a drain ends the watch");

        let later = draining(6, &mut memory);
        let down = later.geyser_command.expect("the app turns it down itself");
        assert_eq!(down, GeyserCommand { intent: GeyserIntent::On(800.0), buttons: vec![Button::W1200] });
    }

    #[test]
    fn a_draining_battery_turns_the_geyser_down_to_what_the_sun_carries() {
        let config = config();
        let both = Running { watts: 2000.0, since: at(12, 0) };
        let down = turn_down_for_drain(&both, 894.0, &config);
        assert_eq!((down.phase, down.watts), (LoadPhase::Stop, Some(800.0)));
        let a_little = turn_down_for_drain(&both, 400.0, &config);
        assert_eq!(a_little.watts, Some(1200.0));
        let low = Running { watts: 800.0, since: at(12, 0) };
        assert_eq!(turn_down_for_drain(&low, 500.0, &config).watts, None, "nothing lower fits: off");
    }

    #[test]
    fn a_full_battery_starts_the_geyser_high_and_steps_it_up_a_level_at_a_time() {
        let config = config();
        let sunny_full = SunNow { spare: 1800.0, full: true, draining: false, explained: true, step_up_ok: true };
        let planned_low = Verdict { phase: LoadPhase::Ready, note: "Turn the geyser on at 800 W.".into(), watts: Some(800.0), slot: None };
        assert_eq!(sun_for_geyser(planned_low, &sunny_full, None, &config).watts, Some(2000.0));

        let running = |watts: f64| Verdict { phase: LoadPhase::Running, note: "Heating.".into(), watts: Some(watts), slot: None };
        let low = Running { watts: 800.0, since: at(11, 0) };
        assert_eq!(sun_for_geyser(running(800.0), &sunny_full, Some(&low), &config).watts, Some(1200.0), "one level at a time");
        let high = Running { watts: 1200.0, since: at(11, 0) };
        assert_eq!(sun_for_geyser(running(1200.0), &sunny_full, Some(&high), &config).watts, Some(2000.0));
        let waiting = SunNow { step_up_ok: false, ..sunny_full };
        assert_eq!(sun_for_geyser(running(800.0), &waiting, Some(&low), &config).watts, Some(800.0));
        let draining = SunNow { draining: true, ..sunny_full };
        assert_eq!(sun_for_geyser(running(800.0), &draining, Some(&low), &config).watts, Some(800.0));

        let mut memory = LoadsMemory::default();
        let mut notify = Vec::new();
        let up = geyser_command(&mut memory, &running(1200.0), Some(&low), Control::Full, at(11, 30), &config, &mut notify).unwrap();
        assert_eq!(up, GeyserCommand { intent: GeyserIntent::On(1200.0), buttons: vec![Button::W1200, Button::W800] });
        assert!(notify[0].1.starts_with("Turning the geyser up to 1200 W"));
    }

    #[test]
    fn a_full_battery_that_isnt_draining_keeps_running_loads_on() {
        let stop = || Verdict { phase: LoadPhase::Stop, note: "Unplug the scooter — the sun can't carry the charger right now.".into(), watts: None, slot: None };
        let kept = sun_carries(stop(), Some(600.0), true);
        assert_eq!((kept.phase, kept.watts), (LoadPhase::Running, Some(600.0)));
        assert_eq!(sun_carries(stop(), Some(600.0), false).phase, LoadPhase::Stop);
        assert_eq!(sun_carries(stop(), None, true).phase, LoadPhase::Stop);

        let charging = sun_covers(stop(), Some(600.0), Some(at(10, 0)), false);
        assert_eq!(charging.phase, LoadPhase::Running);
        assert!(charging.note.contains("fills by about 10:00"), "{}", charging.note);
        assert_eq!(sun_covers(stop(), Some(600.0), None, false).phase, LoadPhase::Stop);
        assert_eq!(sun_covers(stop(), Some(600.0), None, true).phase, LoadPhase::Running);
    }

    #[test]
    fn unused_sun_goes_to_the_geyser_and_keeps_it_on() {
        let config = config();
        let waiting = || Verdict { phase: LoadPhase::Waiting, note: "Best sun for it: 12:00 PM.".into(), watts: Some(800.0), slot: None };
        let full = SunNow { spare: 1500.0, full: true, draining: false, explained: true, step_up_ok: true };
        let now = sun_for_geyser(waiting(), &full, None, &config);
        assert_eq!((now.phase, now.watts), (LoadPhase::Ready, Some(1200.0)));
        assert!(now.note.contains("battery is full"));

        let charging = SunNow { full: false, ..full };
        assert_eq!(sun_for_geyser(waiting(), &charging, None, &config).phase, LoadPhase::Waiting, "only sun the full battery can't take");
        let little = SunNow { spare: 300.0, ..full };
        let unknown = SunNow { explained: false, ..full };
        assert_eq!(sun_for_geyser(waiting(), &unknown, None, &config).phase, LoadPhase::Waiting, "a big load it can't place may be the geyser");
        assert_eq!(sun_for_geyser(waiting(), &little, None, &config).phase, LoadPhase::Waiting);

        let stop = || Verdict { phase: LoadPhase::Stop, note: "Switch the geyser off.".into(), watts: None, slot: None };
        let heating = Running { watts: 1200.0, since: at(11, 0) };
        let carried = sun_for_geyser(stop(), &SunNow { spare: 1300.0, full: false, draining: false, explained: true, step_up_ok: true }, Some(&heating), &config);
        assert_eq!(carried.phase, LoadPhase::Running);
        let drains = sun_for_geyser(stop(), &SunNow { spare: 1300.0, full: false, draining: true, explained: true, step_up_ok: true }, Some(&heating), &config);
        assert_eq!(drains.phase, LoadPhase::Stop);
        let both = Running { watts: 2000.0, since: at(11, 0) };
        assert_eq!(sun_for_geyser(stop(), &SunNow { spare: 1300.0, full: false, draining: false, explained: true, step_up_ok: true }, Some(&both), &config).phase, LoadPhase::Stop);
    }

    #[test]
    fn on_off_is_not_pressed_to_switch_on_while_an_unplaced_load_could_be_the_geyser() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut quiet = running_app();
        assert!(step(&mut quiet, &auto_input(at(11, 0), 300.0, &sun, &config)).geyser_command.is_some());

        let mut restarted = LoadsMemory::default();
        let first = step(&mut restarted, &auto_input(at(11, 0), 1100.0, &sun, &config));
        assert!(first.geyser_command.is_none(), "800 W the app can't place may be the geyser already on");
        let mut ask = auto_input(at(11, 1), 1110.0, &sun, &config);
        ask.geyser_request = Some(GeyserIntent::On(1200.0));
        let manual = step(&mut restarted, &ask);
        assert!(manual.geyser_command.is_none());
        assert!(manual.notify.iter().any(|(_, body)| body.contains("can't place")));
    }

    #[test]
    fn spare_sun_held_off_by_an_unplaced_load_asks_and_a_no_lets_the_geyser_on() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        let first = step(&mut memory, &auto_input(at(12, 40), 850.0, &sun, &config));
        assert!(first.geyser_command.is_none());
        let ask = first.asks.iter().find(|ask| ask.title == HOTPLATE_TITLE).expect("asks about the unplaced load");
        assert!(ask.body.contains("Is the hotplate on?"));
        let again = step(&mut memory, &auto_input(at(12, 45), 860.0, &sun, &config));
        assert!(again.asks.is_empty());

        let mut answered = auto_input(at(12, 46), 860.0, &sun, &config);
        answered.answer = Some(LoadsAnswer::No);
        let on = step(&mut memory, &answered);
        assert!(on.geyser_command.is_some_and(|command| matches!(command.intent, GeyserIntent::On(_))));
    }

    #[test]
    fn the_motor_stopping_beside_the_esp32_geyser_isnt_taken_as_hot_water() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        step(&mut memory, &auto_input(at(13, 22), 830.0, &sun, &config));
        memory.detector.running.insert(LoadKind::Water, Running { watts: 1000.0, since: at(13, 20) });
        memory.detector.running.insert(LoadKind::Geyser, Running { watts: 800.0, since: at(13, 24) });
        step(&mut memory, &auto_input(at(13, 40), 1670.0, &sun, &config));
        memory.geyser_delivered_wh = 2100.0;

        let motor_off = step(&mut memory, &auto_input(at(13, 53), 890.0, &sun, &config));
        assert_eq!(memory.detector.running(LoadKind::Geyser).map(|r| r.watts), Some(800.0), "{:?}", motor_off.log);
        assert!(memory.detector.running(LoadKind::Water).is_none());
        assert!(memory.geyser_heated_at.is_none());
        assert!(motor_off.geyser_command.as_ref().is_none_or(|command| command.intent != GeyserIntent::Off), "{:?}", motor_off.geyser_command);
    }

    #[test]
    fn a_drop_after_switching_an_idle_geyser_off_means_it_was_still_heating() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = running_app();
        memory.detector.running.insert(LoadKind::Geyser, Running { watts: 800.0, since: at(13, 0) });
        step(&mut memory, &auto_input(at(13, 40), 1900.0, &sun, &config));
        memory.geyser_delivered_wh = 2100.0;
        let cut = step(&mut memory, &auto_input(at(13, 53), 1100.0, &sun, &config));
        assert!(memory.geyser_heated_at.is_some(), "{}", cut.log.as_ref().unwrap());
        let command = cut.geyser_command.expect("the idle element is switched off");
        memory.geyser_sent(&command, at(13, 53), true);

        let after = step(&mut memory, &auto_input(at(13, 58), 300.0, &sun, &config));
        assert!(memory.geyser_heated_at.is_none(), "{:?}", after.log);
        assert!(after.notify.iter().any(|(_, body)| body.contains("still heating")));
    }

    fn hot_since_11_33(config: &AutomationConfig, sun: &Forecast) -> LoadsMemory {
        let mut memory = running_app();
        step(&mut memory, &auto_input(at(11, 30), 300.0, sun, config));
        memory.geyser_heated_at = Some(at(11, 33));
        memory.geyser_delivered_wh = 2700.0;
        memory
    }

    #[test]
    fn the_hot_geyser_tops_up_once_on_spare_sun_before_its_window_ends() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = hot_since_11_33(&config, &sun);

        let early = step(&mut memory, &auto_input(at(12, 30), 310.0, &sun, &config));
        assert!(early.geyser_command.is_none());
        assert_eq!(early.status.geyser.row.phase, LoadPhase::Done);
        assert!(early.status.geyser.row.note.as_deref().unwrap_or_default().contains("tops up once more"), "{:?}", early.status.geyser.row.note);

        let on = step(&mut memory, &auto_input(at(13, 10), 320.0, &sun, &config));
        let command = on.geyser_command.clone().unwrap_or_else(|| panic!("{}", on.log.as_ref().unwrap()));
        let GeyserIntent::On(level) = command.intent else { panic!("{command:?}") };
        assert!(on.notify.iter().any(|(_, body)| body.contains("topping it up")));
        memory.geyser_sent(&command, at(13, 10), true);
        step(&mut memory, &auto_input(at(13, 14), 320.0 + level, &sun, &config));
        assert!(memory.detector.running(LoadKind::Geyser).is_some());

        step(&mut memory, &auto_input(at(13, 30), 325.0 + level, &sun, &config));
        let cut = step(&mut memory, &auto_input(at(13,  45), 330.0, &sun, &config));
        assert!(memory.geyser.reheated_at.is_some(), "{}", cut.log.as_ref().unwrap());
        assert!(cut.notify.iter().any(|(_, body)| body.starts_with("Topped up")));
        let off = cut.geyser_command.expect("the elements are switched off after the top-up");
        assert_eq!(off.intent, GeyserIntent::Off);
        memory.geyser_sent(&off, at(13, 45), true);

        let later = step(&mut memory, &auto_input(at(14, 30), 340.0, &sun, &config));
        assert!(later.geyser_command.is_none());
        assert_eq!(later.status.geyser.row.phase, LoadPhase::Done);
        assert!(later.status.geyser.row.note.as_deref().unwrap_or_default().contains("again at 1:45 PM"), "{:?}", later.status.geyser.row.note);
    }

    #[test]
    fn a_top_up_press_that_draws_nothing_switches_the_elements_back_off() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = hot_since_11_33(&config, &sun);
        let on = step(&mut memory, &auto_input(at(13, 10), 320.0, &sun, &config));
        let command = on.geyser_command.unwrap();
        memory.geyser_sent(&command, at(13, 10), true);

        let none = step(&mut memory, &auto_input(at(13, 14), 325.0, &sun, &config));
        assert!(none.geyser_command.is_none());
        assert!(none.notify.iter().any(|(_, body)| body.contains("still hot")));
        assert!(memory.geyser.failed_on.is_none());

        let off = step(&mut memory, &auto_input(at(13, 20), 330.0, &sun, &config)).geyser_command.expect("the elements go back off");
        assert_eq!(off.intent, GeyserIntent::Off);
        assert_eq!(off.buttons, command.buttons);
    }

    #[test]
    fn the_top_up_waits_while_a_big_load_is_on() {
        let config = AutomationConfig { geyser_auto_control: true, geyser_esp32_host: "geyser".into(), ..config() };
        let sun = sunny(800.0);
        let mut memory = hot_since_11_33(&config, &sun);
        memory.detector.running.insert(LoadKind::Water, Running { watts: 750.0, since: at(13, 0) });
        let busy = step(&mut memory, &auto_input(at(13, 10), 1070.0, &sun, &config));
        assert!(busy.geyser_command.is_none(), "{:?}", busy.geyser_command);
        assert_eq!(busy.status.geyser.row.phase, LoadPhase::Waiting);
        assert!(busy.status.geyser.row.note.as_deref().unwrap_or_default().contains("Tops it up once more"));
    }

    #[test]
    fn a_turn_down_toggles_one_element() {
        let config = config();
        let down = Verdict { phase: LoadPhase::Stop, note: "Clouds.".into(), watts: Some(1200.0), slot: None };
        let both = Running { watts: 2000.0, since: at(12, 0) };
        let mut memory = LoadsMemory::default();
        let mut notify = Vec::new();
        let command = geyser_command(&mut memory, &down, Some(&both), Control::Full, at(12, 30), &config, &mut notify).unwrap();
        assert_eq!(command, GeyserCommand { intent: GeyserIntent::On(1200.0), buttons: vec![Button::W800] });
        assert!(notify[0].1.starts_with("Turning the geyser down to 1200 W"));
    }
}
