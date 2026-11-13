//! The AI decides the solar loads: what is running, today's runs and the
//! notes the family reads. Nothing it says is used until the checks here pass.

use chrono::NaiveTime;

use super::*;
use crate::automation::agent::{EvDayRow, LoadsAiInput, LoadsBelief, LoadsDecision, LoadsHour, LoadsNotes, LoadsReading};

const RECHECK_MINUTES: (f64, f64) = (10.0, 30.0);
const LOAD_CHANGE_W: f64 = 300.0;
/// A change in the load asks again at most this often; the hotplate's level
/// changes would otherwise use up the model's rate limit.
const LOAD_ASK_GAP_MINUTES: i64 = 10;
/// A step this far over the spare sun is where a plan breaks the battery.
const OVERRUN_W: f64 = 100.0;
const RETRY_MINUTES: i64 = 10;
const RECENT_MINUTES: i64 = 90;
/// The house alone may read a little under zero once the loads are taken
/// off: the loads' real draw wanders around their labels.
const RESIDUAL_SLACK_W: f64 = 150.0;
const RESIDUAL_ROOM_W: f64 = 400.0;
const LATE_END_MINUTES: i64 = 15;
/// What the family set by hand binds the AI until the load moves this much
/// or this long has passed.
const PIN_CHANGE_W: f64 = 400.0;
const PIN_MINUTES: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    #[default]
    Off,
    Ai,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct AiStatus {
    pub mode: AiMode,
    pub at: Option<NaiveDateTime>,
    pub reason: Option<String>,
    pub model: Option<String>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadsQuestion {
    pub text: String,
    pub at: NaiveDateTime,
}

#[derive(Debug, Default)]
pub(super) struct AiMemory {
    pub mode: AiMode,
    pub decided_at: Option<NaiveDateTime>,
    pub slots: Vec<Slot>,
    pub notes: LoadsNotes,
    pub reason: Option<String>,
    pub model: Option<String>,
    pub problems: Vec<String>,
    pub next_at: Option<NaiveDateTime>,
    pub retry_at: Option<NaiveDateTime>,
    pub asked_at: Option<NaiveDateTime>,
    pub asked_load_w: Option<f64>,
    pub asked_state: Option<String>,
    /// The question, and the loads the AI newly believed in when it asked.
    pub question: Option<(LoadsQuestion, Vec<LoadKind>)>,
    pub question_notified: bool,
    pub answers: Vec<String>,
    /// The family's state when the decision was made: a change since makes
    /// it stale until the AI answers again.
    pub decided_state: Option<String>,
    pub pins: Vec<Pin>,
}

/// A load the family said is on or off, with the house load at that moment.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pin {
    kind: LoadKind,
    on: bool,
    at: NaiveDateTime,
    load_w: Option<f64>,
}

impl Pin {
    fn holds(&self, now: NaiveDateTime, load_w: Option<f64>) -> bool {
        let moved = matches!((load_w, self.load_w), (Some(now_w), Some(then_w)) if (now_w - then_w).abs() >= PIN_CHANGE_W);
        now - self.at < Duration::minutes(PIN_MINUTES) && !moved
    }
}

/// A decision that passed every check, ready to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct Verified {
    pub running: Vec<(LoadKind, f64, NaiveDateTime)>,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone)]
enum Levels {
    Exact(Vec<f64>),
    Near(f64),
    Range(f64, f64),
}

#[derive(Debug, Clone)]
struct Rule {
    allowed: Result<(), String>,
    /// Why this load has to be in today's plan, when it has to.
    required: Option<String>,
    /// It runs whatever the sun, so the grid may top it up.
    topped_up: bool,
    levels: Levels,
    from: NaiveDateTime,
    until: NaiveDateTime,
}

/// What the checks need from the moment the AI was asked.
#[derive(Debug, Clone)]
pub struct Checker {
    now: NaiveDateTime,
    soc: f64,
    rated_ah: f64,
    volts: f64,
    max_charge_a: Option<f64>,
    steps: Vec<Step>,
    target: f64,
    low: f64,
    load_w: Option<f64>,
    house_usual_w: f64,
    rules: BTreeMap<LoadKind, Rule>,
    pins: Vec<(LoadKind, bool)>,
    water_run: Duration,
    /// What the app's own plan takes from the grid for the required loads.
    app_grid_wh: Option<f64>,
}

pub(super) fn load_name(kind: LoadKind) -> &'static str {
    match kind {
        LoadKind::Water => "water",
        LoadKind::Geyser => "geyser",
        LoadKind::Ev => "scooter",
        LoadKind::Hotplate => "hotplate",
    }
}

fn load_kind(name: &str) -> Option<LoadKind> {
    match name.trim().to_ascii_lowercase().as_str() {
        "water" | "motor" | "water tank" => Some(LoadKind::Water),
        "geyser" => Some(LoadKind::Geyser),
        "scooter" | "ev" => Some(LoadKind::Ev),
        "hotplate" | "choola" => Some(LoadKind::Hotplate),
        _ => None,
    }
}

fn no_sun(_: NaiveDateTime) -> Option<f64> {
    None
}

fn no_house(_: u32) -> Option<f64> {
    None
}

fn hhmm(at: NaiveDateTime) -> String {
    at.format("%H:%M").to_string()
}

/// The prompt asks for 24-hour HH:MM, but models sometimes write seconds or
/// "1:30 PM"; any of those is clear enough.
fn parse_hhmm(today: NaiveDate, text: &str) -> Option<NaiveDateTime> {
    let text = text.trim().to_ascii_uppercase();
    ["%H:%M", "%H:%M:%S", "%I:%M %p", "%I:%M%p"]
        .iter()
        .find_map(|format| NaiveTime::parse_from_str(&text, format).ok())
        .map(|time| today.and_time(time))
}

pub(super) fn extra_for(steps: &[Step], slots: &[Slot]) -> Vec<f64> {
    steps
        .iter()
        .map(|step| slots.iter().filter(|slot| step.start < slot.end && step.end() > slot.start).map(|slot| slot.watts).sum())
        .collect()
}

/// What the required loads alone take from the grid.
pub(super) fn required_grid_wh(sun: &Sun, slots: &[Slot], required: &[LoadKind]) -> f64 {
    let steps = steps(sun);
    let baseline = simulate(sun, &steps, &vec![0.0; steps.len()]);
    let low = sun.day_low.max(sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc);
    let slots: Vec<Slot> = slots.iter().filter(|slot| required.contains(&slot.kind)).cloned().collect();
    simulate_hybrid(sun, &steps, &extra_for(&steps, &slots), low).grid_wh
}

/// The AI's runs, simulated like the app's own plan.
pub(super) fn plan_from_slots(sun: &Sun, slots: &[Slot]) -> Option<Plan> {
    if sun.volts <= 0.0 || sun.rated_ah <= 0.0 {
        return None;
    }
    let steps = steps(sun);
    if steps.is_empty() {
        return None;
    }
    let baseline = simulate(sun, &steps, &vec![0.0; steps.len()]);
    let low = sun.day_low.max(sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc);
    let extra = extra_for(&steps, slots);
    let run = simulate(sun, &steps, &extra);
    let hybrid = simulate_hybrid(sun, &steps, &extra, low);
    let cap_wh = sun.rated_ah * sun.volts;
    let spare_wh = (run.curtailed_wh + (run.end_soc - baseline.end_soc) / 100.0 * cap_wh).max(0.0);
    let slots: Vec<Slot> = slots
        .iter()
        .filter(|slot| slot.end > sun.now)
        .map(|slot| Slot {
            topped_up: steps.iter().zip(&hybrid.grid_steps).any(|(step, grid)| *grid && step.start < slot.end && step.end() > slot.start),
            ..slot.clone()
        })
        .collect();
    let topped_up = slots.iter().any(|slot| slot.topped_up);
    Some(Plan {
        slots,
        sun_only_soc: baseline.end_soc,
        end_soc: if topped_up { hybrid.end_soc } else { run.end_soc },
        full_at: if topped_up { hybrid.full_at } else { run.full_at },
        spare_wh,
        spare_before_ev_wh: spare_wh,
        grid_wh: hybrid.grid_wh,
        grid_now: hybrid.grid_now,
    })
}

impl Levels {
    fn problem(&self, kind: LoadKind, watts: f64) -> Option<String> {
        let name = load_name(kind);
        let ok = match self {
            Levels::Exact(levels) => levels.iter().any(|level| (level - watts).abs() <= 60.0),
            Levels::Near(level) => (level - watts).abs() <= level * 0.25,
            Levels::Range(low, high) => (*low..=*high).contains(&watts),
        };
        (!ok).then(|| match self {
            Levels::Exact(levels) => {
                let list: Vec<String> = levels.iter().map(|level| format!("{level:.0}")).collect();
                format!("The {name} runs at exactly {} W, not {watts:.0} W.", list.join(", "))
            }
            Levels::Near(level) => format!("The {name} draws about {level:.0} W, not {watts:.0} W."),
            Levels::Range(low, high) => format!("The {name} runs between {low:.0} and {high:.0} W, not {watts:.0} W."),
        })
    }
}

impl Checker {
    /// A load already on can't be undone at once: the motor fills the tank,
    /// anything else needs someone to walk over and switch it off.
    fn committed_until(&self, kind: LoadKind, since: NaiveDateTime) -> NaiveDateTime {
        let next_step = self.now + Duration::minutes(STEP_MINUTES);
        match kind {
            LoadKind::Water => (since + self.water_run).max(next_step),
            _ => next_step,
        }
    }

    fn sun(&self) -> Sun<'static> {
        Sun {
            now: self.now,
            sunset: self.now,
            soc: self.soc,
            rated_ah: self.rated_ah,
            volts: self.volts,
            max_charge_a: self.max_charge_a,
            floor: 0.0,
            day_low: 0.0,
            pv_now_w: None,
            house_now_w: None,
            pv_at: &no_sun,
            house_at: &no_house,
        }
    }

    pub fn check(&self, decision: &LoadsDecision) -> Result<Verified, Vec<String>> {
        let today = self.now.date();
        let mut problems = Vec::new();
        let mut running: Vec<(LoadKind, f64, NaiveDateTime)> = Vec::new();
        for item in &decision.running {
            let Some(kind) = load_kind(&item.load) else {
                problems.push(format!("\"{}\" isn't one of the loads (water, geyser, hotplate, scooter).", item.load));
                continue;
            };
            if running.iter().any(|(listed, _, _)| *listed == kind) {
                problems.push(format!("The {} is listed twice as running.", load_name(kind)));
                continue;
            }
            if let Some(problem) = self.rules[&kind].levels.problem(kind, item.watts) {
                problems.push(problem);
                continue;
            }
            let since = parse_hhmm(today, &item.since)
                .filter(|at| *at <= self.now + Duration::minutes(1) && *at >= today.and_time(NaiveTime::MIN))
                .unwrap_or(self.now);
            running.push((kind, item.watts, since));
        }
        for (kind, on) in &self.pins {
            let listed = running.iter().any(|(listed, _, _)| listed == kind);
            if *on && !listed {
                problems.push(format!("The family says the {} is on; keep it in running.", load_name(*kind)));
            } else if !*on && listed {
                problems.push(format!("The family says the {} is off; leave it out of running.", load_name(*kind)));
            }
        }
        if let Some(load) = self.load_w {
            let claimed: f64 = running.iter().map(|(_, watts, _)| watts).sum();
            let rest = load - claimed;
            let most = self.house_usual_w + (self.house_usual_w * 0.5).max(RESIDUAL_ROOM_W);
            if rest < -RESIDUAL_SLACK_W {
                problems.push(format!(
                    "The running loads add up to {claimed:.0} W, more than the {load:.0} W the house is using now."
                ));
            } else if rest > most {
                problems.push(format!(
                    "The house is using {load:.0} W. The running loads explain {claimed:.0} W, leaving {rest:.0} W, but the house alone usually uses about {:.0} W now, so something that isn't listed is on.",
                    self.house_usual_w
                ));
            }
        }

        let mut slots: Vec<Slot> = Vec::new();
        for run in &decision.plan {
            let Some(kind) = load_kind(&run.load) else {
                problems.push(format!("\"{}\" in the plan isn't one of the loads.", run.load));
                continue;
            };
            let name = load_name(kind);
            let rule = &self.rules[&kind];
            if slots.iter().any(|slot| slot.kind == kind) {
                problems.push(format!("The {name} has more than one run; give each load one run."));
                continue;
            }
            let is_running = running.iter().any(|(listed, _, _)| *listed == kind);
            if let (Err(why), false) = (&rule.allowed, is_running) {
                problems.push(format!("The {name} shouldn't run today: {why}."));
                continue;
            }
            let (Some(start), Some(end)) = (parse_hhmm(today, &run.start), parse_hhmm(today, &run.end)) else {
                problems.push(format!("The {name}'s run needs start and end as HH:MM."));
                continue;
            };
            let start = start.max(self.now);
            if end <= start {
                problems.push(format!("The {name}'s run ends at {} — already past or before it starts.", hhmm(end)));
                continue;
            }
            if !is_running && start + Duration::minutes(1) < rule.from {
                problems.push(format!("The {name} can't start before {}.", hhmm(rule.from)));
            }
            if end > rule.until + Duration::minutes(LATE_END_MINUTES) {
                problems.push(format!("The {name} must be off by {}.", hhmm(rule.until)));
            }
            if let Some(problem) = rule.levels.problem(kind, run.watts) {
                problems.push(problem);
                continue;
            }
            slots.push(Slot { kind, start, end: end.min(rule.until.max(start)), watts: run.watts, partial: false, topped_up: false });
        }

        for (kind, rule) in &self.rules {
            let running = running.iter().any(|(listed, _, _)| listed == kind);
            if let (Some(why), false, false) = (&rule.required, running, slots.iter().any(|slot| slot.kind == *kind)) {
                let verb = if *kind == LoadKind::Geyser { "heat" } else { "charge" };
                problems.push(format!("The {} must {verb} today ({why}); give it a run.", load_name(*kind)));
            }
        }
        let committed: Vec<Slot> = slots
            .iter()
            .filter_map(|slot| {
                if self.rules[&slot.kind].topped_up {
                    return Some(slot.clone());
                }
                let (_, _, since) = running.iter().find(|(kind, _, _)| *kind == slot.kind)?;
                let until = self.committed_until(slot.kind, *since);
                (until > slot.start).then(|| Slot { end: slot.end.min(until), ..slot.clone() })
            })
            .collect();
        let sun = self.sun();
        let held = extra_for(&self.steps, &committed);
        let with_held = simulate(&sun, &self.steps, &held);
        let low = self.low.min(with_held.low_soc);
        let target = self.target.min(with_held.end_soc);
        let extra = extra_for(&self.steps, &slots);
        let simulated = simulate(&sun, &self.steps, &extra);
        let short = simulated.low_soc < low - 0.5 || simulated.end_soc < target - 0.5;
        let overrun = self
            .steps
            .iter()
            .zip(extra.iter().zip(&held))
            .find(|(step, (runs, held))| **runs > **held + 1.0 && **runs - step.surplus_w > OVERRUN_W)
            .map(|(step, (runs, _))| (step, runs));
        if let Some((step, runs)) = overrun.filter(|_| short) {
            problems.push(format!(
                "From {} the runs take {:.0} W but only {:.0} W of sun is spare after the house; shorten, lower or move a run.",
                hhmm(step.start),
                runs,
                step.surplus_w.max(0.0)
            ));
        }
        if simulated.low_soc < low - 0.5 {
            problems.push(format!(
                "With this plan the battery would drop to {:.0}% in the day; it mustn't go below {:.0}%.",
                simulated.low_soc, low
            ));
        }
        if simulated.end_soc < target - 0.5 {
            problems.push(format!(
                "With this plan the battery would end the day at {:.0}% instead of the {:.0}% it reaches with only what is on now, so it wouldn't be full again before the sun fades.",
                simulated.end_soc, target
            ));
        }
        if let Some(app_grid) = self.app_grid_wh {
            let required: Vec<Slot> = slots.iter().filter(|slot| self.rules[&slot.kind].topped_up).cloned().collect();
            let grid = simulate_hybrid(&sun, &self.steps, &extra_for(&self.steps, &required), self.low).grid_wh;
            if grid > app_grid + (app_grid * 0.2).max(200.0) {
                problems.push(format!(
                    "This plan takes about {:.1} kWh from the grid; with the required loads in the sunniest hours it needs only about {:.1} kWh. Move them to where the sun is strongest.",
                    grid / 1000.0,
                    app_grid / 1000.0
                ));
            }
        }
        if problems.is_empty() {
            Ok(Verified { running, slots })
        } else {
            Err(problems)
        }
    }
}

/// Everything about the moment the AI is asked, gathered by `step`.
pub(super) struct Facts<'a> {
    pub input: &'a LoadsInput<'a>,
    pub sun: &'a Sun<'a>,
    pub memory: &'a LoadsMemory,
    pub house_usual_w: f64,
    pub pv_held_back: bool,
    pub water_running: Option<&'a Running>,
    pub geyser_running: Option<&'a Running>,
    pub ev_running: Option<&'a Running>,
    pub hotplate_running: Option<Running>,
    pub geyser_done: bool,
    pub ev_skipped: bool,
    pub hotplate_skipped: bool,
    pub cooking_day: bool,
    pub cooked_today: bool,
    pub geyser_first: Option<&'a str>,
    pub next_days: Vec<EvDayRow>,
    pub ev_rule: Option<&'a EvToday>,
    pub geyser_required: Option<&'a str>,
    pub app_grid_wh: Option<f64>,
}

pub(super) fn checker(facts: &Facts) -> Checker {
    let Facts { input, sun, .. } = facts;
    let config = input.config;
    let today = input.now.date();
    let steps = steps(sun);
    let baseline = simulate(sun, &steps, &vec![0.0; steps.len()]);
    let low = sun.day_low.max(sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc);
    let window = |start: u32, end: u32| (at_hour(today, start), at_hour(today, end).min(input.sunset));
    let mut rules = BTreeMap::new();
    let (from, until) = window(config.water_start_hour, config.water_end_hour);
    let water_allowed = if !config.water_reminder_enabled {
        Err("water reminders are off".to_string())
    } else if !input.water_pending {
        Err("the tank is done for today".to_string())
    } else {
        Ok(())
    };
    rules.insert(LoadKind::Water, Rule { required: None, topped_up: false, allowed: water_allowed, levels: Levels::Near(config.water_motor_watts), from, until });
    let (from, until) = window(config.geyser_start_hour, config.geyser_end_hour);
    let geyser_allowed = if !config.geyser_enabled {
        Err("geyser planning is off".to_string())
    } else if facts.geyser_done || facts.memory.geyser_heated_at.is_some() {
        Err("the water is already hot today".to_string())
    } else {
        Ok(())
    };
    let geyser_required = facts.geyser_required.filter(|_| geyser_allowed.is_ok()).map(str::to_string);
    rules.insert(
        LoadKind::Geyser,
        Rule {
            topped_up: geyser_required.is_some(),
            required: geyser_required,
            allowed: geyser_allowed,
            levels: Levels::Exact(geyser_levels(config)),
            from,
            until,
        },
    );
    let (from, until) = window(config.hotplate_start_hour, config.hotplate_end_hour);
    let hotplate_allowed = if !config.hotplate_enabled {
        Err("hotplate planning is off".to_string())
    } else if facts.hotplate_skipped {
        Err("the family isn't cooking on the hotplate today".to_string())
    } else if facts.cooked_today && facts.hotplate_running.is_none() {
        Err("they have cooked today already".to_string())
    } else if !facts.cooking_day {
        Err("it isn't a cooking day".to_string())
    } else {
        Ok(())
    };
    rules.insert(LoadKind::Hotplate, Rule { required: None, topped_up: false, allowed: hotplate_allowed, levels: Levels::Range(100.0, HOTPLATE_MAX_W + 100.0), from, until });
    let ev_soc = facts.memory.ev.soc.as_ref().map(|soc| soc.soc);
    let ev_allowed = if !config.ev_enabled {
        Err("scooter planning is off".to_string())
    } else if facts.ev_skipped {
        Err("the family skipped it today".to_string())
    } else if ev_soc.is_some_and(|soc| soc >= FULL_SOC) {
        Err("it is full".to_string())
    } else if ev_soc.is_none() {
        Err("its charge isn't known yet".to_string())
    } else if let Some(EvToday::NotNeeded(why)) = facts.ev_rule {
        Err(format!("no charge needed today ({why})"))
    } else {
        Ok(())
    };
    let ev_required = match facts.ev_rule {
        Some(EvToday::Needed(why)) if ev_allowed.is_ok() => Some(why.clone()),
        _ => None,
    };
    rules.insert(
        LoadKind::Ev,
        Rule {
            allowed: ev_allowed,
            topped_up: ev_required.is_some(),
            required: ev_required,
            levels: Levels::Near(config.ev_charger_watts),
            from: input.now,
            until: input.sunset,
        },
    );
    Checker {
        now: input.now,
        soc: sun.soc,
        rated_ah: sun.rated_ah,
        volts: sun.volts,
        max_charge_a: sun.max_charge_a,
        target: baseline.end_soc,
        low,
        steps,
        load_w: input.load_w,
        house_usual_w: facts.house_usual_w,
        rules,
        pins: facts
            .memory
            .ai
            .pins
            .iter()
            .filter(|pin| pin.holds(input.now, input.load_w))
            .map(|pin| (pin.kind, pin.on))
            .collect(),
        water_run: Duration::minutes(config.water_run_minutes as i64),
        app_grid_wh: facts.app_grid_wh,
    }
}

fn running_text(running: Option<&Running>, label: &str) -> String {
    running.map_or(String::new(), |r| format!(" The app believes it is on at {:.0} W since {}{label}.", r.watts, hhmm(r.since)))
}

pub(super) fn ai_input(facts: &Facts) -> LoadsAiInput {
    let Facts { input, sun, memory, .. } = facts;
    let config = input.config;
    let now = input.now;
    let today = now.date();
    let window = |start: u32, end: u32| format!("{:02}:00–{:02}:00", start, end);
    let steps = steps(sun);
    let baseline = simulate(sun, &steps, &vec![0.0; steps.len()]);
    let low = sun.day_low.max(sun.floor + FLOOR_MARGIN_SOC).min(baseline.low_soc);

    let since = now - Duration::minutes(RECENT_MINUTES);
    let mut recent: Vec<LoadsReading> = input
        .recent
        .iter()
        .filter(|sample| sample.at >= since && sample.at < now)
        .map(|sample| LoadsReading { time: hhmm(sample.at), load_w: sample.load_w, pv_w: sample.pv_w, soc: sample.soc })
        .collect();
    recent.push(LoadsReading { time: hhmm(now), load_w: input.load_w, pv_w: input.pv_w, soc: input.soc });

    let pinned = |kind: LoadKind| memory.ai.pins.iter().rev().find(|pin| pin.kind == kind && pin.holds(now, input.load_w)).map(|pin| pin.on);
    let mut believed: Vec<LoadsBelief> = [
        (LoadKind::Water, facts.water_running),
        (LoadKind::Geyser, facts.geyser_running),
        (LoadKind::Ev, facts.ev_running),
        (LoadKind::Hotplate, facts.hotplate_running.as_ref()),
    ]
    .into_iter()
    .filter(|(kind, _)| pinned(*kind) != Some(false))
    .filter_map(|(kind, running)| running.map(|r| LoadsBelief { load: load_name(kind).into(), watts: r.watts.round(), since: hhmm(r.since) }))
    .collect();
    believed.sort_by(|a, b| a.since.cmp(&b.since));
    let belief = |kind: LoadKind, running: Option<&Running>, label: &str| {
        match memory.ai.pins.iter().rev().find(|pin| pin.kind == kind && pin.holds(now, input.load_w)) {
            Some(pin) if pin.on => format!(" The family set it ON at {}: list it in running.", hhmm(pin.at)),
            Some(pin) => format!(" The family set it OFF at {}: leave it out of running, even if the load looks high.", hhmm(pin.at)),
            None => running_text(running, label),
        }
    };

    let pv_at = sun.pv_at;
    let hours = (now.hour() + 1..=input.sunset.hour())
        .map(|hour| at_hour(today, hour))
        .filter(|at| *at < input.sunset)
        .map(|at| LoadsHour {
            time: hhmm(at),
            sun_w: ((0..4).filter_map(|quarter| pv_at(at + Duration::minutes(15 * quarter))).sum::<f64>() / 4.0).round(),
            house_w: (sun.house_at)(at.hour()).unwrap_or(facts.house_usual_w).round(),
        })
        .collect();

    let water = if !config.water_reminder_enabled {
        "Reminders are off; leave it out.".to_string()
    } else if input.water_pending || facts.water_running.is_some() {
        format!(
            "Needs filling today: {:.0} W for about {} min, between {}.{}",
            config.water_motor_watts,
            config.water_run_minutes,
            window(config.water_start_hour, config.water_end_hour),
            belief(LoadKind::Water, facts.water_running, "")
        )
    } else {
        "Filled for today; leave it out.".to_string()
    };

    let geyser = if !config.geyser_enabled {
        "Planning is off; leave it out.".to_string()
    } else {
        let state = match (facts.geyser_done, memory.geyser_heated_at) {
            (_, Some(at)) => format!(" Its thermostat cut it out at {} — the water is hot.", hhmm(at)),
            (true, None) => " The family marked the water hot enough today.".to_string(),
            _ => String::new(),
        };
        let required = match facts.geyser_required.filter(|_| state.is_empty()) {
            Some(why) => format!(" Required today ({why}): give it its full run even if the sun can't carry it; the grid tops up the rest."),
            None => String::new(),
        };
        format!(
            "Storage water heater with two elements: runs at exactly {} W ({:.0} = both). Wants about {:.1} kWh a day; {:.1} kWh delivered so far today. Between {}, never after sunset. Its thermostat cuts it out when the water is hot.{state}{required}{}",
            geyser_levels(config).iter().rev().map(|w| format!("{w:.0}")).collect::<Vec<_>>().join(", "),
            config.geyser_low_watts + config.geyser_high_watts,
            config.geyser_daily_kwh,
            memory.geyser_delivered_wh / 1000.0,
            window(config.geyser_start_hour, config.geyser_end_hour),
            belief(LoadKind::Geyser, facts.geyser_running, "")
        )
    };

    let hotplate = if !config.hotplate_enabled {
        "Planning is off; leave it out.".to_string()
    } else {
        let last = input.hotplate_on.map_or("not recorded yet".to_string(), |day| day.format("%a %d %b").to_string());
        let today_text = if facts.hotplate_skipped {
            "The family said they aren't cooking on it today."
        } else if facts.cooked_today && facts.hotplate_running.is_none() {
            "They cooked on it today already."
        } else if facts.cooking_day {
            "Today is a cooking day."
        } else {
            "Today isn't a cooking day, but if the readings show it on, it is on."
        };
        format!(
            "Electric cooker: any level 100–2200 W, usually about {:.0} W for {:.1} h, between {}, every {} days (last cooked: {last}). {today_text} With too little sun they cook on gas instead.{}",
            config.hotplate_watts,
            config.hotplate_hours,
            window(config.hotplate_start_hour, config.hotplate_end_hour),
            config.hotplate_every_days,
            belief(LoadKind::Hotplate, facts.hotplate_running.as_ref(), " (its level is the load above the house)")
        )
    };

    let scooter = if !config.ev_enabled {
        "Planning is off; leave it out.".to_string()
    } else {
        match memory.ev.soc.as_ref() {
            None => "Its charge isn't known yet; leave it out.".to_string(),
            Some(soc) => {
                let basis = match soc.basis {
                    SocBasis::Read => "read over Bluetooth: the value it held when last switched off",
                    SocBasis::Live => "read live",
                    SocBasis::Estimated => "estimated from the charger running",
                    SocBasis::Full => "the charger switched itself off",
                    SocBasis::Set => "set by the family",
                };
                let need = ev_need_wh(soc.soc, config);
                format!(
                    "Yadea T5L, {:.2} kWh: {:.0}% ({basis}, {}). Needs {:.1} kWh ({:.1} h on its {:.0} W charger) to reach 100%; a day's riding uses about {:.0}% ({:.1} days left). Solar only, never the grid; charges to 100%.{}{}{}",
                    config.ev_battery_kwh,
                    soc.soc,
                    hhmm(soc.at),
                    need / 1000.0,
                    need / config.ev_charger_watts,
                    config.ev_charger_watts,
                    config.ev_daily_use_pct,
                    soc.soc / config.ev_daily_use_pct,
                    if facts.ev_skipped { " The family skipped it today; leave it out." } else { "" },
                    match facts.ev_rule {
                        Some(EvToday::Needed(why)) => format!(
                            " Required today — the family's rule says charge it today ({why}): give it its run even if the sun can't carry it all; the grid tops up the rest."
                        ),
                        Some(EvToday::NotNeeded(why)) => {
                            format!(" The family's rule says no charge is needed today ({why}): leave it out unless it's already charging.")
                        }
                        None => String::new(),
                    },
                    belief(LoadKind::Ev, facts.ev_running, "")
                )
            }
        }
    };

    let priority = match facts.geyser_first {
        Some(why) => format!("Today the geyser goes before the hotplate ({why}), then the scooter."),
        None => "Usually the hotplate first, then the geyser, then the scooter.".to_string(),
    };

    LoadsAiInput {
        now: now.format("%A %d %b %Y").to_string(),
        time: hhmm(now),
        sunset: hhmm(input.sunset),
        soc: sun.soc.round(),
        battery_kwh: (sun.rated_ah * sun.volts / 100.0).round() / 10.0,
        battery_lend_kwh: ((sun.soc - low).max(0.0) / 100.0 * sun.rated_ah * sun.volts / 100.0).round() / 10.0,
        battery_w: input.charge_a.map(|amps| (amps * sun.volts).round()),
        day_floor_soc: low.round(),
        full_by_sun_alone: baseline.full_at.map(hhmm),
        sunset_soc_sun_alone: baseline.end_soc.round(),
        load_w: input.load_w.map(f64::round),
        pv_w: input.pv_w.map(f64::round),
        pv_held_back: facts.pv_held_back,
        sky_vs_forecast_pct: input.sky_factor.map(|factor| (factor * 100.0).round()),
        house_usual_w: facts.house_usual_w.round(),
        recent,
        believed,
        hours,
        water,
        geyser,
        hotplate,
        scooter,
        priority,
        next_days: facts.next_days.clone(),
        answers: memory.ai.answers.clone(),
        previous: memory.ai.reason.as_ref().zip(memory.ai.decided_at).map(|(reason, at)| format!("{}: {reason}", hhmm(at))),
        problems: Vec::new(),
    }
}

impl AiMemory {
    pub fn due(&self, now: NaiveDateTime, load_w: Option<f64>, state: &str) -> bool {
        if self.retry_at.is_some_and(|at| now < at) {
            return false;
        }
        self.decided_at.is_none_or(|at| at.date() != now.date())
            || self.next_at.is_none_or(|at| now >= at)
            || (matches!((load_w, self.asked_load_w), (Some(now_w), Some(then_w)) if (now_w - then_w).abs() >= LOAD_CHANGE_W)
                && self.asked_at.is_none_or(|at| now - at >= Duration::minutes(LOAD_ASK_GAP_MINUTES)))
            || self.asked_state.as_deref() != Some(state)
    }

    pub fn live(&self, now: NaiveDateTime, state: &str) -> bool {
        self.mode == AiMode::Ai && self.decided_at.is_some_and(|at| at.date() == now.date()) && self.decided_state.as_deref() == Some(state)
    }

    pub fn note(&self, kind: LoadKind) -> Option<&str> {
        let note = match kind {
            LoadKind::Water => &self.notes.water,
            LoadKind::Geyser => &self.notes.geyser,
            LoadKind::Hotplate => &self.notes.hotplate,
            LoadKind::Ev => &self.notes.scooter,
        };
        Some(note.trim()).filter(|note| !note.is_empty())
    }

    pub fn status(&self) -> AiStatus {
        AiStatus {
            mode: self.mode,
            at: self.decided_at,
            reason: self.reason.clone(),
            model: self.model.clone(),
            problems: self.problems.clone(),
        }
    }
}

impl LoadsMemory {
    pub fn ai_decided_today(&self, now: NaiveDateTime) -> bool {
        self.ai.decided_at.is_some_and(|at| at.date() == now.date())
    }

    pub fn scooter_first_today(&self, now: NaiveDateTime) -> Option<bool> {
        self.ev.decision.as_ref().filter(|d| d.at.date() == now.date()).map(|d| d.charge == EvCharge::Yes)
    }

    pub fn ai_live(&self, now: NaiveDateTime) -> bool {
        self.ai.mode == AiMode::Ai && self.ai.decided_at.is_some_and(|at| at.date() == now.date())
    }

    /// What the AI was asked with, so a big change in the load or a button
    /// press asks again.
    pub fn ai_asked(&mut self, now: NaiveDateTime, load_w: Option<f64>, state: String) {
        self.ai.asked_at = Some(now);
        self.ai.asked_load_w = load_w;
        self.ai.asked_state = Some(state);
        self.ai.next_at = Some(now + Duration::minutes(RECHECK_MINUTES.1 as i64));
    }

    pub fn ai_failed(&mut self, now: NaiveDateTime) {
        self.ai.retry_at = Some(now + Duration::minutes(RETRY_MINUTES));
    }

    /// Both answers failed the checks: the app's own plan runs until the
    /// next time the AI is asked.
    pub fn ai_rejected(&mut self, now: NaiveDateTime, problems: Vec<String>, reason: String, model: Option<String>) {
        self.ai.mode = AiMode::Fallback;
        self.ai.decided_at = Some(now);
        self.ai.problems = problems;
        self.ai.reason = Some(reason);
        self.ai.model = model;
        self.ai.slots.clear();
    }

    pub fn apply_ai(&mut self, verified: Verified, decision: &LoadsDecision, load_w: Option<f64>, now: NaiveDateTime) {
        let before = self.detector.running.clone();
        let had_hotplate = self.hotplate.running.is_some();
        self.detector.running.clear();
        let mut hotplate = None;
        let mut new_kinds = Vec::new();
        for (kind, watts, since) in &verified.running {
            let known = before.contains_key(kind) || (*kind == LoadKind::Hotplate && had_hotplate);
            if !known {
                new_kinds.push(*kind);
            }
            if *kind == LoadKind::Hotplate {
                hotplate = Some((*watts, *since));
                continue;
            }
            let since = before.get(kind).map_or(*since, |running| running.since.min(*since));
            if *kind == LoadKind::Geyser && !before.contains_key(kind) {
                self.geyser_delivered_wh += watts * (now - since).num_seconds().max(0) as f64 / 3600.0;
            }
            self.detector.running.insert(*kind, Running { watts: *watts, since });
        }
        self.hotplate.running = hotplate.map(|(watts, since)| {
            let others = self.detector.running_w();
            let earlier = self.hotplate.running.as_ref();
            HotplateRun {
                since: earlier.map_or(since, |run| run.since.min(since)),
                base_w: load_w.map_or(0.0, |load| (load - watts - others).max(0.0)),
                believed_at: earlier.map_or(now, |run| run.believed_at),
                confirmed: earlier.is_some_and(|run| run.confirmed),
            }
        });
        self.hotplate.asking = None;
        self.hotplate.quiet_since = None;

        let question = decision.question.trim();
        let asks = !decision.sure && !question.is_empty();
        let same_question = self.ai.question.as_ref().is_some_and(|(q, _)| q.text == question);
        if asks && !same_question {
            self.ai.question = Some((LoadsQuestion { text: question.to_string(), at: now }, new_kinds));
            self.ai.question_notified = false;
        } else if !asks {
            self.ai.question = None;
        }

        let soc = self.ev.soc.as_ref().map_or(0.0, |soc| soc.soc);
        let scooter_reason = Some(decision.notes.scooter.trim()).filter(|note| !note.is_empty()).unwrap_or(decision.reason.trim());
        self.ev.decision = Some(EvDecision {
            charge: if decision.scooter_first { EvCharge::Yes } else { EvCharge::IfSpare },
            reason: scooter_reason.to_string(),
            model: decision.model.clone(),
            at: now,
            soc,
            recheck_at: now + Duration::minutes(RECHECK_MINUTES.1 as i64),
        });

        let recheck = if decision.recheck_minutes.is_finite() { decision.recheck_minutes } else { RECHECK_MINUTES.1 };
        self.ai.mode = AiMode::Ai;
        self.ai.decided_at = Some(now);
        self.ai.decided_state = self.ai.asked_state.clone();
        self.ai.slots = verified.slots;
        self.ai.notes = decision.notes.clone();
        self.ai.reason = Some(decision.reason.clone());
        self.ai.model = decision.model.clone();
        self.ai.problems.clear();
        self.ai.retry_at = None;
        self.ai.next_at = Some(now + Duration::minutes(recheck.clamp(RECHECK_MINUTES.0, RECHECK_MINUTES.1).round() as i64));
    }

    /// The family setting a load by hand: applied at once, told to the AI as
    /// fact, and held against its answers for a while.
    pub fn apply_edit(&mut self, edit: &LoadEdit, now: NaiveDateTime, load_w: Option<f64>, config: &AutomationConfig) {
        let on = edit.state == EditState::On;
        let watts = edit.watts.filter(|watts| watts.is_finite() && *watts > 0.0);
        if edit.load == LoadKind::Hotplate {
            self.hotplate.asking = None;
            self.hotplate.quiet_since = None;
            self.hotplate.running = on.then(|| {
                let watts = watts.unwrap_or(config.hotplate_watts);
                let others = self.detector.running_w();
                let since = self.hotplate.running.as_ref().map_or(now, |run| run.since);
                HotplateRun { since, base_w: load_w.map_or(0.0, |load| (load - watts - others).max(0.0)), believed_at: now, confirmed: true }
            });
        } else if on {
            let usual = match edit.load {
                LoadKind::Water => config.water_motor_watts,
                LoadKind::Geyser => config.geyser_high_watts.max(config.geyser_low_watts),
                _ => config.ev_charger_watts,
            };
            let since = self.detector.running.get(&edit.load).map_or(now, |running| running.since);
            self.detector.running.insert(edit.load, Running { watts: watts.unwrap_or(usual), since });
        } else {
            self.detector.running.remove(&edit.load);
        }
        if edit.load == LoadKind::Water {
            self.water_ask = None;
            self.water_seen_since = None;
        }
        if edit.load == LoadKind::Geyser {
            if edit.state != EditState::Done {
                self.geyser_heated_at = None;
            }
            if let Some(kwh) = edit.kwh.filter(|kwh| kwh.is_finite() && *kwh >= 0.0) {
                self.geyser_delivered_wh = kwh * 1000.0;
            }
        }
        if let Some(soc) = edit.soc.filter(|soc| (0.0..=100.0).contains(soc)).filter(|_| edit.load == LoadKind::Ev) {
            self.ev.soc = Some(EvSoc { soc, at: now, basis: SocBasis::Set });
        }

        let name = load_name(edit.load);
        let level = |usual: &str| watts.map_or(usual.to_string(), |watts| format!("on at {watts:.0} W"));
        let what = match (edit.load, edit.state) {
            (LoadKind::Water, EditState::On) => level("on"),
            (LoadKind::Water, EditState::Off) => "off, and the tank isn't filled yet".into(),
            (LoadKind::Water, EditState::Done) => "off: the tank is filled for today".into(),
            (LoadKind::Geyser, EditState::On) => level("on"),
            (LoadKind::Geyser, EditState::Off) => "off: switched off by hand, the water isn't hot yet".into(),
            (LoadKind::Geyser, EditState::Done) => "off: the water is hot enough for today".into(),
            (LoadKind::Hotplate, EditState::On) => level("on"),
            (LoadKind::Hotplate, EditState::Off) => "off: they haven't cooked on it today".into(),
            (LoadKind::Hotplate, EditState::Done) => "off: they cooked on it today and are finished".into(),
            (LoadKind::Ev, EditState::On) => "charging".into(),
            (LoadKind::Ev, _) => "not charging".into(),
        };
        let mut said = format!("{}: the family set the {name} to {what}", hhmm(now));
        if let (LoadKind::Geyser, Some(kwh)) = (edit.load, edit.kwh) {
            said.push_str(&format!(", {kwh:.1} kWh heated so far today"));
        }
        if let (LoadKind::Ev, Some(soc)) = (edit.load, edit.soc) {
            said.push_str(&format!(", at {soc:.0}%"));
        }
        self.tell_ai(edit.load, on, said, now, load_w);
    }

    /// Two live reads of the scooter's own %: a rise means the charger is on,
    /// no change or a drop means it isn't.
    pub(super) fn ev_charge_seen(&mut self, seen: ChargeSeen, now: NaiveDateTime, load_w: Option<f64>, config: &AutomationConfig) {
        let rise = seen.to - seen.from;
        let charging = rise >= EV_RISE_PCT && seen.to < FULL_SOC;
        let span = match seen.from_at {
            Some(from_at) => format!("{:.1}% at {} → {:.1}% at {}", seen.from, hhmm(from_at), seen.to, hhmm(seen.at)),
            None => format!("{:.1}% last known → {:.1}% read live at {}", seen.from, seen.to, hhmm(seen.at)),
        };
        let check = if charging {
            format!("Charging: {span}")
        } else if seen.to >= FULL_SOC {
            format!("Full: {span}")
        } else if rise <= -EV_RISE_PCT {
            format!("Not charging, it was ridden: {span}")
        } else {
            format!("Not charging: {span}")
        };
        if charging {
            let hours = rise / ev_rate_pct_per_hour(config);
            let since = seen.at - Duration::seconds((hours * 3600.0).round() as i64);
            let since = seen.from_at.map_or(since, |from_at| since.max(from_at));
            let since = self.detector.running.get(&LoadKind::Ev).map_or(since, |running| running.since.min(since));
            self.detector.running.insert(LoadKind::Ev, Running { watts: config.ev_charger_watts, since });
        } else {
            self.detector.running.remove(&LoadKind::Ev);
        }
        let said = format!("{}: the scooter's own % read {span}, so it {}", hhmm(now), if charging { "is charging" } else { "isn't charging" });
        self.ev.charge_check = Some(check);
        self.tell_ai(LoadKind::Ev, charging, said, now, load_w);
    }

    /// Something the app knows for sure goes to the AI as fact and holds its
    /// answers to it for a while.
    fn tell_ai(&mut self, kind: LoadKind, on: bool, said: String, now: NaiveDateTime, load_w: Option<f64>) {
        self.ai.answers.push(said);
        self.ai.pins.retain(|pin| pin.kind != kind);
        self.ai.pins.push(Pin { kind, on, at: now, load_w });
        if self.ai.question.as_ref().is_some_and(|(_, guessed)| guessed.contains(&kind)) {
            self.ai.question = None;
        }
        self.ai.next_at = Some(now);
    }

    /// The family's yes or no to the AI's question goes into what it's told
    /// next time; a no also drops what it had guessed.
    pub(super) fn answer_ai(&mut self, yes: bool, now: NaiveDateTime) {
        let Some((question, guessed)) = self.ai.question.take() else {
            return;
        };
        self.ai.answers.push(format!("{}: asked \"{}\", the family said {}", hhmm(now), question.text, if yes { "yes" } else { "no" }));
        if !yes {
            for kind in guessed {
                if kind == LoadKind::Hotplate {
                    self.hotplate.running = None;
                } else {
                    self.detector.running.remove(&kind);
                }
            }
        } else if let Some(run) = self.hotplate.running.as_mut().filter(|_| guessed.contains(&LoadKind::Hotplate)) {
            run.confirmed = true;
        }
        self.ai.next_at = Some(now);
    }
}
