use std::time::{Duration, Instant};

use chrono::Timelike;
use tauri::AppHandle;

use crate::automation::config::{hour_in_window, hours_until, AutomationConfig};
use crate::automation::{
    AutomationState, AutomationWarning, Estimate, EngineMemory, Phase, discharge_average,
    push_discharge_sample,
};
use crate::battery::BatteryState;
use crate::battery::types::BatterySnapshot;
use crate::inverter::client::SolarClient;
use crate::inverter::types::{InverterSnapshot, output_mode_name};
use crate::inverter::InverterError;
use crate::notifications::{Notifier, Reading};
use crate::storage::load_automation_config;

const SBG_MODE: u32 = 1;
const PROBE_DELAY_SECONDS: u64 = 150;
const WRITE_COOLDOWN: Duration = Duration::from_secs(600);
const BACKOFF_EXTRA_MARGIN_HOURS: f64 = 1.0;
const INVERTER_EFFICIENCY: f64 = 0.9;

pub struct BatteryTelemetry {
    pub soc: f64,
    pub discharge_a: f64,
    pub usable_capacity_ah: f64,
    pub source: &'static str,
}

pub struct EngineReading {
    pub soc: Option<f64>,
    pub discharge_a: f64,
    pub current_a: Option<f64>,
    pub usable_capacity_ah: f64,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub mode: Option<u32>,
    pub smart_load: Option<u32>,
    pub batt_v: Option<f64>,
    pub grid_on: Option<bool>,
    pub source: &'static str,
}

impl EngineReading {
    async fn from_snapshot(
        snapshot: &InverterSnapshot,
        battery: &BatteryState,
        config: &AutomationConfig,
    ) -> Self {
        let mode = snapshot.settings.as_ref().and_then(|settings| settings.output_source_priority_value);
        let smart_load = snapshot.settings.as_ref().and_then(|settings| settings.smart_load);
        let pv_w = snapshot.pv_watts();
        let load_w = snapshot.load_watts();
        let batt_v = snapshot.field(&["batteryVoltage", "battery_voltage"]);
        let grid_on = snapshot.grid_on();

        let bms = if battery.connection_status().await.connected {
            battery.latest_snapshot().await
        } else {
            None
        };

        let Some(telemetry) = resolve_telemetry(bms.as_ref(), snapshot, config) else {
            return Self {
                soc: None,
                discharge_a: 0.0,
                current_a: None,
                usable_capacity_ah: 0.0,
                pv_w,
                load_w,
                mode,
                smart_load,
                batt_v,
                grid_on,
                source: "inverter",
            };
        };

        let current_a = bms
            .as_ref()
            .map(|bms| bms.current)
            .or_else(|| Some(-telemetry.discharge_a));
        Self {
            soc: Some(telemetry.soc),
            discharge_a: telemetry.discharge_a,
            current_a,
            usable_capacity_ah: telemetry.usable_capacity_ah,
            pv_w,
            load_w,
            mode,
            smart_load,
            batt_v,
            grid_on,
            source: telemetry.source,
        }
    }
}

pub fn spawn_engine(
    state: AutomationState,
    client: SolarClient,
    battery: BatteryState,
    notifier: Notifier,
    app: AppHandle,
) {
    tauri::async_runtime::spawn(async move {
        let persisted = load_automation_config(&app);
        *state.config.lock().await = persisted.clone();
        state
            .set_status(|status| status.enabled = persisted.enabled)
            .await;
        state.log("Automation engine started").await;
        publish_status(&state, &app).await;

        let mut check_rx = state.check_tx.subscribe();
        loop {
            let config = state.config().await;
            let full_interval = Duration::from_secs(
                config.check_interval_minutes.saturating_mul(60),
            );
            let probe_due = state
                .memory
                .lock()
                .await
                .probe_until
                .filter(|until| *until > Instant::now());
            let sleep = match probe_due {
                Some(until) => until
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_secs(1)),
                None => full_interval,
            };
            let next_check = chrono::Local::now()
                + chrono::Duration::from_std(sleep).unwrap_or_default();
            state
                .set_status(|status| status.next_check = Some(next_check.to_rfc3339()))
                .await;

            let _ = check_rx.borrow_and_update();
            tokio::select! {
                _ = tokio::time::sleep(sleep) => {}
                _ = check_rx.changed() => {
                    continue;
                }
            }

            state
                .set_status(|status| status.last_check = Some(chrono::Local::now().to_rfc3339()))
                .await;
            if let Err(error) = evaluate(&state, &client, &battery, &notifier).await {
                state
                    .log_change(format!("Automation check failed: {error}"))
                    .await;
            }
            publish_status(&state, &app).await;
        }
    });
}

pub async fn evaluate(
    state: &AutomationState,
    client: &SolarClient,
    battery: &BatteryState,
    notifier: &Notifier,
) -> Result<(), InverterError> {
    let config = state.config().await;
    let hour = chrono::Local::now().hour();

    let snapshot = client.read_inverter_snapshot(None).await?;
    let reading = EngineReading::from_snapshot(&snapshot, battery, &config).await;
    let grid_on = snapshot.grid_on();

    let mut notif_reading = Reading {
        grid_on,
        ..Reading::default()
    };
    if reading.source == "inverter" {
        notif_reading.soc = reading.soc;
        notif_reading.current_a = reading.current_a;
    }
    notifier.observe(notif_reading).await;

    state
        .set_status(|status| {
            status.current_mode_name = reading.mode.map(output_mode_name);
            status.data_source = Some(reading.source.to_string());
            status.live.soc = reading.soc;
            status.live.battery_current_a = reading.current_a;
            status.live.pv_w = reading.pv_w;
            status.live.load_w = reading.load_w;
            status.live.grid_on = grid_on;
            status.live.smart_load = reading.smart_load;
        })
        .await;

    if !config.enabled {
        return Ok(());
    }

    if hour_in_window(hour, config.window_start_hour, config.window_end_hour) {
        night_step(state, client, &reading, &config, hour, notifier).await?;
    } else {
        day_step(state, client, &reading, &config, notifier).await?;
    }

    let engaged = state.memory.lock().await.previous_mode.is_some();
    state
        .set_status(|status| status.automation_engaged = engaged)
        .await;
    Ok(())
}

fn resolve_telemetry(
    bms: Option<&BatterySnapshot>,
    snapshot: &InverterSnapshot,
    config: &AutomationConfig,
) -> Option<BatteryTelemetry> {
    if let Some(bms) = bms {
        let discharge_a = if bms.current < 0.0 { -bms.current } else { 0.0 };
        let usable_capacity_ah =
            (bms.remaining_capacity - bms.rated_capacity * config.reserve_soc_percent / 100.0)
                .max(0.0);
        return Some(BatteryTelemetry {
            soc: bms.soc as f64,
            discharge_a,
            usable_capacity_ah,
            source: "bms",
        });
    }

    let soc = snapshot.field(&["batterySOC"])?;
    let discharge_a = snapshot
        .field(&["batteryDischargeCurrent"])
        .filter(|current| *current >= 0.0)
        .unwrap_or(0.0);
    let capacity_ah = snapshot
        .field(&["batteryCapacity"])
        .unwrap_or(config.capacity_ah);
    let usable_capacity_ah =
        ((soc - config.reserve_soc_percent) * capacity_ah / 100.0).max(0.0);
    Some(BatteryTelemetry {
        soc,
        discharge_a,
        usable_capacity_ah,
        source: "inverter",
    })
}

fn project_runtime(usable_capacity_ah: f64, discharge_a: f64) -> Option<f64> {
    if discharge_a > 0.0 {
        Some((usable_capacity_ah / discharge_a).max(0.0))
    } else if usable_capacity_ah > 0.0 {
        None
    } else {
        Some(0.0)
    }
}

/// `None` runtime means "indefinite" and is treated as covering the gap.
fn covers(runtime: Option<f64>, required: f64) -> bool {
    runtime.is_none_or(|hours| hours >= required)
}

fn estimate_discharge_a(load_w: Option<f64>, pv_w: Option<f64>, batt_v: Option<f64>) -> Option<f64> {
    let load = load_w?;
    let voltage = batt_v.filter(|voltage| *voltage > 0.0)?;
    let pv = pv_w.unwrap_or(0.0);
    Some(((load - pv).max(0.0)) / voltage / INVERTER_EFFICIENCY)
}

/// Day rule: revert from an SBG hold, then switch Solar → and only Solar →
/// once PV is weak and the battery is actually draining.
fn should_switch_to_solar(
    grid_on: Option<bool>,
    last_write_at: Option<Instant>,
    mode: Option<u32>,
    pv_w: Option<f64>,
    discharge_a: f64,
    config: &AutomationConfig,
) -> bool {
    if grid_on == Some(false) {
        return false;
    }
    if last_write_at.is_some_and(|written| written.elapsed() < WRITE_COOLDOWN) {
        return false;
    }
    mode == Some(SBG_MODE)
        && pv_w.unwrap_or(0.0) < config.day_pv_threshold_watts
        && discharge_a > config.day_discharge_threshold_a
}

async fn day_step(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    notifier: &Notifier,
) -> Result<(), InverterError> {
    let holding_from_night = {
        let mut memory = state.memory.lock().await;
        let holding = memory.previous_mode.is_some()
            || matches!(memory.phase, Phase::Probing | Phase::Holding | Phase::BackedOff);
        if !holding {
            memory.phase = Phase::Day;
        }
        holding
    };
    if !holding_from_night {
        return day_rule(state, client, reading, config, notifier).await;
    }

    let previous = {
        let memory = state.memory.lock().await;
        memory.previous_mode.unwrap_or(0)
    };
    client.write_output_priority(None, previous.to_string()).await?;

    let mut memory = state.memory.lock().await;
    memory.phase = Phase::Day;
    memory.previous_mode = None;
    memory.samples.clear();
    memory.probe_until = None;
    memory.engaged_at = None;
    memory.consecutive_shortfalls = 0;
    memory.last_write_at = Some(Instant::now());
    drop(memory);
    state
        .set_status(|status| {
            status.automation_engaged = false;
            status.warning = None;
            status.estimate = Estimate::default();
            status.verified = Estimate::default();
            status.required_h = None;
        })
        .await;
    state
        .log("Left the night window — restored previous output mode")
        .await;
    notifier.send(
        "Automation",
        "Left the night window — restored previous output mode",
    );
    Ok(())
}

async fn day_rule(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    notifier: &Notifier,
) -> Result<(), InverterError> {
    let last_write_at = state.memory.lock().await.last_write_at;
    if !should_switch_to_solar(
        reading.grid_on,
        last_write_at,
        reading.mode,
        reading.pv_w,
        reading.discharge_a,
        config,
    ) {
        return Ok(());
    }

    client.write_output_priority(None, "solar".into()).await?;
    state.memory.lock().await.last_write_at = Some(Instant::now());
    state.memory.lock().await.samples.clear();
    state
        .set_status(|status| {
            status.current_mode_name = Some(output_mode_name(0));
            status.warning = None;
        })
        .await;
    let message = format!(
        "PV low ({} W) and battery draining ({} A) — switched from SBG to Solar",
        reading.pv_w.unwrap_or(0.0),
        reading.discharge_a
    );
    state.log(&message).await;
    notifier.send("Automation", &message);
    Ok(())
}

async fn night_step(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    hour: u32,
    notifier: &Notifier,
) -> Result<(), InverterError> {
    let hours_to_target = hours_until(hour, config.target_hour);
    let required_hours = hours_to_target + config.safety_margin_hours;
    state
        .set_status(|status| status.required_h = Some(required_hours))
        .await;

    let mut memory = state.memory.lock().await;

    if memory.previous_mode.is_some() && reading.mode != Some(SBG_MODE) {
        if memory.phase != Phase::UserOverride {
            memory.phase = Phase::UserOverride;
            memory.previous_mode = None;
            memory.samples.clear();
            memory.probe_until = None;
            memory.engaged_at = None;
            memory.consecutive_shortfalls = 0;
            state
                .set_status(|status| status.automation_engaged = false)
                .await;
            state
                .log("You changed the output mode — automation paused for the rest of the night")
                .await;
            notifier.send(
                "Automation",
                "You changed the output mode — automation paused for the rest of the night",
            );
        }
        return Ok(());
    }

    if let Some(soc) = reading.soc {
        if soc < config.min_soc_percent {
            if memory.phase != Phase::LowSoc {
                memory.phase = Phase::LowSoc;
                memory.probe_until = None;
                memory.engaged_at = None;
                memory.consecutive_shortfalls = 0;
                let restored = memory.previous_mode.is_some();
                if let Some(previous) = memory.previous_mode.take() {
                    client.write_output_priority(None, previous.to_string()).await?;
                    memory.last_write_at = Some(Instant::now());
                }
                memory.samples.clear();
                state
                    .set_status(|status| status.automation_engaged = false)
                    .await;
                state
                    .log(format!(
                        "SOC {soc:.0}% is below the {:.0}% floor — {}",
                        config.min_soc_percent,
                        if restored {
                            "restored previous output mode"
                        } else {
                            "staying on current mode"
                        }
                    ))
                    .await;
            }
            return Ok(());
        }
    }

    // First tick of the window enters Waiting.
    if memory.phase == Phase::Day {
        memory.phase = Phase::Waiting;
    }

    match memory.phase {
        Phase::Waiting => {
            night_waiting(state, client, reading, config, &mut memory, notifier, required_hours).await
        }
        Phase::Probing => {
            night_probing(state, client, reading, config, &mut memory, notifier, required_hours).await
        }
        Phase::Holding => {
            night_holding(state, client, reading, config, &mut memory, notifier, required_hours).await
        }
        Phase::BackedOff => {
            night_backed_off(state, client, reading, &mut memory, notifier, required_hours)
                .await
        }
        Phase::UserOverride | Phase::LowSoc | Phase::Day => Ok(()),
    }
}

fn estimate_text(discharge: Option<f64>, runtime: Option<f64>) -> String {
    match discharge {
        Some(amps) => {
            let runtime = runtime
                .map(|hours| format!("{hours:.1}h"))
                .unwrap_or_else(|| "indefinite".to_string());
            format!("{amps:.1} A → {runtime}")
        }
        None => "no load data".to_string(),
    }
}

async fn night_waiting(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    required_hours: f64,
) -> Result<(), InverterError> {
    // User already set SBG themselves: keep today's behaviour — leave it alone
    // if it covers the gap, otherwise step back to Solar.
    if reading.mode == Some(SBG_MODE) {
        let runtime = project_runtime(reading.usable_capacity_ah, reading.discharge_a);
        if !covers(runtime, required_hours)
            && !within_tolerance(reading.soc, runtime, required_hours, config)
        {
            client.write_output_priority(None, "solar".into()).await?;
            memory.last_write_at = Some(Instant::now());
            memory.samples.clear();
            let message = format!(
                "SBG would not last until {:02}:00 — user had set SBG; switched to Solar",
                config.target_hour
            );
            state.log(&message).await;
            state
                .set_status(|status| {
                    status.current_mode_name = Some(output_mode_name(0));
                    status.warning = Some(AutomationWarning {
                        timestamp: chrono::Local::now().to_rfc3339(),
                        message,
                    });
                })
                .await;
            notifier.send(
                "Automation",
                "User-set SBG could not cover the night — switched to Solar",
            );
        }
        return Ok(());
    }

    let estimate_a = estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v);
    let estimate_runtime = estimate_a.and_then(|amps| project_runtime(reading.usable_capacity_ah, amps));
    let text = estimate_text(estimate_a, estimate_runtime);
    state
        .set_status(|status| {
            status.estimate = Estimate {
                discharge_a: estimate_a,
                runtime_h: estimate_runtime,
            };
            status.verified = Estimate::default();
        })
        .await;

    let probe_without_data = estimate_a.is_none();
    if probe_without_data || covers(estimate_runtime, required_hours) {
        engage_sbg(state, client, reading, memory, notifier, &text, "covers the night").await?;
    } else {
        memory.probe_until = None;
        state
            .log_change(format!(
                "Estimate {text} won't cover the {required_hours:.1}h until {:02}:00 — staying on Solar",
                config.target_hour
            ))
            .await;
    }
    Ok(())
}

async fn engage_sbg(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    text: &str,
    reason: &str,
) -> Result<(), InverterError> {
    memory.previous_mode = reading.mode.or(Some(0));
    client.write_output_priority(None, "sbg".into()).await?;
    memory.last_write_at = Some(Instant::now());
    if reading.smart_load == Some(0) {
        if let Err(error) = client.write_smart_load(None, true).await {
            tracing::warn!("automation could not enable smart load: {error}");
        } else {
            memory.last_write_at = Some(Instant::now());
        }
    }
    memory.phase = Phase::Probing;
    memory.probe_until = Some(Instant::now() + Duration::from_secs(PROBE_DELAY_SECONDS));
    memory.samples.clear();
    memory.engaged_at = Some(Instant::now());
    memory.consecutive_shortfalls = 0;
    state
        .set_status(|status| {
            status.automation_engaged = true;
            status.warning = None;
        })
        .await;
    state
        .log(format!(
            "Switched to SBG — {reason} (estimated {text}), probing real discharge"
        ))
        .await;
    notifier.send(
        "Automation",
        &format!("Switched to SBG for the night (estimated {text})"),
    );
    Ok(())
}

fn shortfall_hours(verified_runtime: Option<f64>, required_hours: f64) -> f64 {
    match verified_runtime {
        None => 0.0,
        Some(runtime) => (required_hours - runtime).max(0.0),
    }
}

fn within_tolerance(
    soc: Option<f64>,
    verified_runtime: Option<f64>,
    required_hours: f64,
    config: &AutomationConfig,
) -> bool {
    if covers(verified_runtime, required_hours) {
        return true;
    }
    let shortfall = shortfall_hours(verified_runtime, required_hours);
    if shortfall > config.deficit_tolerance_hours {
        return false;
    }
    match soc {
        Some(soc) => soc >= config.high_soc_hold_percent,
        None => shortfall <= config.deficit_tolerance_hours / 2.0,
    }
}

fn min_hold_elapsed(memory: &EngineMemory, config: &AutomationConfig) -> bool {
    match memory.engaged_at {
        None => true,
        Some(engaged) => engaged.elapsed() >= Duration::from_secs(config.min_hold_minutes * 60),
    }
}

async fn night_probing(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    required_hours: f64,
) -> Result<(), InverterError> {
    if reading.mode == Some(SBG_MODE) && reading.discharge_a > 0.0 {
        push_discharge_sample(&mut memory.samples, reading.discharge_a);
    }
    let verified_a = discharge_average(&memory.samples);
    let verified_runtime = if memory.samples.is_empty() {
        None
    } else {
        project_runtime(reading.usable_capacity_ah, verified_a)
    };
    state
        .set_status(|status| {
            status.verified = Estimate {
                discharge_a: if memory.samples.is_empty() {
                    None
                } else {
                    Some(verified_a)
                },
                runtime_h: verified_runtime,
            };
        })
        .await;

    let probe_pending = memory
        .probe_until
        .is_some_and(|until| until > Instant::now());
    let need_samples = (config.probe_required_samples as usize).max(1);
    if probe_pending || memory.samples.len() < need_samples {
        if !probe_pending {
            memory.probe_until = None;
        }
        state
            .log_change(format!(
                "Probing SBG discharge ({}/{need_samples} samples, {verified_a:.1} A) — holding while verifying",
                memory.samples.len()
            ))
            .await;
        return Ok(());
    }
    memory.probe_until = None;

    if covers(verified_runtime, required_hours) {
        memory.phase = Phase::Holding;
        memory.consecutive_shortfalls = 0;
        state
            .log(format!(
                "Verified {verified_a:.1} A → {:.1}h — holding SBG until {:02}:00",
                verified_runtime.unwrap_or(0.0),
                config.target_hour
            ))
            .await;
        return Ok(());
    }

    if within_tolerance(reading.soc, verified_runtime, required_hours, config) {
        memory.phase = Phase::Holding;
        memory.consecutive_shortfalls = 0;
        let shortfall = shortfall_hours(verified_runtime, required_hours);
        state
            .log(format!(
                "Verified {verified_a:.1} A → {:.1}h ({shortfall:.1}h short, SOC {:.0}%) — holding SBG, load usually drops later",
                verified_runtime.unwrap_or(0.0),
                reading.soc.unwrap_or(0.0),
            ))
            .await;
        return Ok(());
    }

    back_off(state, client, memory, notifier, verified_a, verified_runtime, None).await?;
    Ok(())
}

async fn night_holding(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    config: &AutomationConfig,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    required_hours: f64,
) -> Result<(), InverterError> {
    if reading.mode == Some(SBG_MODE) && reading.discharge_a > 0.0 {
        push_discharge_sample(&mut memory.samples, reading.discharge_a);
    }
    let verified_a = discharge_average(&memory.samples);
    let verified_runtime = project_runtime(reading.usable_capacity_ah, verified_a);
    state
        .set_status(|status| {
            status.verified = Estimate {
                discharge_a: Some(verified_a),
                runtime_h: verified_runtime,
            };
        })
        .await;

    if covers(verified_runtime, required_hours) {
        memory.consecutive_shortfalls = 0;
        return Ok(());
    }

    if within_tolerance(reading.soc, verified_runtime, required_hours, config) {
        memory.consecutive_shortfalls = 0;
        state
            .log_change(format!(
                "Verified {verified_a:.1} A → {:.1}h ({:.1}h short) — holding SBG, load usually drops later",
                verified_runtime.unwrap_or(0.0),
                shortfall_hours(verified_runtime, required_hours),
            ))
            .await;
        return Ok(());
    }

    if !min_hold_elapsed(memory, config) {
        state
            .log_change(format!(
                "Verified {verified_a:.1} A → {:.1}h — in min-hold, staying on SBG",
                verified_runtime.unwrap_or(0.0),
            ))
            .await;
        return Ok(());
    }

    memory.consecutive_shortfalls += 1;
    if memory.consecutive_shortfalls < config.hold_failures_before_revert.max(1) {
        state
            .log_change(format!(
                "Verified {verified_a:.1} A → {:.1}h — short {}/{} checks, watching before reverting",
                verified_runtime.unwrap_or(0.0),
                memory.consecutive_shortfalls,
                config.hold_failures_before_revert.max(1),
            ))
            .await;
        return Ok(());
    }

    back_off(
        state,
        client,
        memory,
        notifier,
        verified_a,
        verified_runtime,
        Some(format!(
            "battery is draining faster than expected ({verified_a:.1} A, {} straight checks)",
            memory.consecutive_shortfalls
        )),
    )
    .await?;
    Ok(())
}

async fn night_backed_off(
    state: &AutomationState,
    client: &SolarClient,
    reading: &EngineReading,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    required_hours: f64,
) -> Result<(), InverterError> {
    let estimate_a = estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v);
    let estimate_runtime = estimate_a.and_then(|amps| project_runtime(reading.usable_capacity_ah, amps));
    state
        .set_status(|status| {
            status.estimate = Estimate {
                discharge_a: estimate_a,
                runtime_h: estimate_runtime,
            };
        })
        .await;

    if estimate_a.is_none() || !covers(estimate_runtime, required_hours + BACKOFF_EXTRA_MARGIN_HOURS) {
        return Ok(());
    }

    let text = estimate_text(estimate_a, estimate_runtime);
    engage_sbg(state, client, reading, memory, notifier, &text, "load dropped").await?;
    Ok(())
}

/// Reverts to the previous mode and parks the state machine in BackedOff.
async fn back_off(
    state: &AutomationState,
    client: &SolarClient,
    memory: &mut EngineMemory,
    notifier: &Notifier,
    verified_a: f64,
    verified_runtime: Option<f64>,
    reason: Option<String>,
) -> Result<(), InverterError> {
    if let Some(previous) = memory.previous_mode.take() {
        client.write_output_priority(None, previous.to_string()).await?;
        memory.last_write_at = Some(Instant::now());
    }
    memory.phase = Phase::BackedOff;
    memory.samples.clear();
    memory.engaged_at = None;
    memory.consecutive_shortfalls = 0;
    memory.probe_until = None;
    let runtime = verified_runtime
        .map(|hours| format!("{hours:.1}h"))
        .unwrap_or_else(|| "0.0h".to_string());
    let detail = reason.unwrap_or_else(|| format!("only {runtime} left"));
    let message = format!("Verified {verified_a:.1} A → {runtime}; {detail} — reverted out of SBG");
    state
        .set_status(|status| {
            status.automation_engaged = false;
            status.warning = Some(AutomationWarning {
                timestamp: chrono::Local::now().to_rfc3339(),
                message: message.clone(),
            });
        })
        .await;
    state.log(&message).await;
    notifier.send("Automation", &message);
    Ok(())
}

async fn publish_status(state: &AutomationState, app: &AppHandle) {
    let status = state.status().await;
    crate::events::emit(app, crate::events::AUTOMATION_STATUS, status);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::config::AutomationConfig;
    use crate::inverter::types::InverterSettings;
    use serde_json::{Value, json};

    fn flow(watts: f64, unit: &str) -> Value {
        json!({ "value": { "value": watts, "unit": unit } })
    }

    fn snapshot_with(soc: f64, discharge_a: f64, capacity_ah: f64, mode: u32) -> InverterSnapshot {
        snapshot_with_flows(soc, discharge_a, capacity_ah, mode, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot_with_flows(
        soc: f64,
        discharge_a: f64,
        capacity_ah: f64,
        mode: u32,
        pv_w: Option<f64>,
        load_w: Option<f64>,
        ac_input_voltage: Option<f64>,
    ) -> InverterSnapshot {
        let mut fields = serde_json::Map::new();
        fields.insert("batterySOC".into(), json!({"value": soc.to_string()}));
        fields.insert(
            "batteryDischargeCurrent".into(),
            json!({"value": discharge_a.to_string()}),
        );
        fields.insert("batteryCapacity".into(), json!({"value": capacity_ah.to_string()}));
        if let Some(voltage) = ac_input_voltage {
            fields.insert("acInputVoltage".into(), json!({"value": voltage.to_string()}));
        }
        InverterSnapshot {
            device_id: "test".into(),
            fields,
            groups: vec![],
            firing_alarms: vec![],
            settings: Some(InverterSettings {
                output_source_priority: output_mode_name(mode),
                output_source_priority_value: Some(mode),
                charger_source_priority: "Solar + Utility".into(),
                charger_source_priority_value: Some(0),
                ac_input_range: None,
                battery_power_limiting: None,
                low_battery_cutoff_voltage: None,
                high_cutoff_voltage: None,
                low_dc_cutoff_soc: None,
                max_total_charge_current: None,
                max_utility_charge_current: None,
                smart_load: None,
            }),
            pv_panel_flow: pv_w.map(|watts| flow(watts, "W")),
            grid_flow: None,
            load_flow: load_w.map(|watts| flow(watts, "W")),
        }
    }

    fn bms_with(soc: u8, current: f64, remaining: f64, rated: f64) -> BatterySnapshot {
        let mut bms = BatterySnapshot::new("bms".into(), "test battery".into());
        bms.soc = soc;
        bms.current = current;
        bms.remaining_capacity = remaining;
        bms.rated_capacity = rated;
        bms
    }

    #[test]
    fn snapshot_reads_latest_field_values() {
        let snapshot = snapshot_with(80.0, 20.0, 100.0, 0);
        assert_eq!(snapshot.field(&["batterySOC"]), Some(80.0));
        assert_eq!(snapshot.field(&["batteryDischargeCurrent"]), Some(20.0));
        assert_eq!(snapshot.field(&["batteryCapacity"]), Some(100.0));
    }

    #[test]
    fn flow_watts_reads_scalar_and_kilowatt_flows() {
        let snapshot = snapshot_with_flows(80.0, 0.0, 100.0, 0, Some(1500.0), Some(200.0), Some(230.0));
        assert_eq!(snapshot.pv_watts(), Some(1500.0));
        assert_eq!(snapshot.load_watts(), Some(200.0));
        assert_eq!(snapshot.grid_on(), Some(true));

        let kilowatt = InverterSnapshot {
            pv_panel_flow: Some(flow(1.5, "kW")),
            ..snapshot.clone()
        };
        assert_eq!(kilowatt.pv_watts(), Some(1500.0));
    }

    #[test]
    fn grid_on_flags_mains_below_100_volts() {
        assert_eq!(snapshot_with_flows(80.0, 0.0, 100.0, 0, None, None, Some(0.0)).grid_on(), Some(false));
        assert_eq!(snapshot_with_flows(80.0, 0.0, 100.0, 0, None, None, Some(231.0)).grid_on(), Some(true));
        assert_eq!(snapshot_with(80.0, 0.0, 100.0, 0).grid_on(), None);
    }

    #[test]
    fn night_hold_runtime_math_is_well_defined() {
        let config = AutomationConfig {
            reserve_soc_percent: 15.0,
            capacity_ah: 100.0,
            ..AutomationConfig::default()
        };
        let usable = ((80.0 - config.reserve_soc_percent) * config.capacity_ah / 100.0).max(0.0);
        assert!((usable - 65.0).abs() < 1e-9);
        let runtime = project_runtime(usable, 20.0).unwrap();
        assert!((runtime - 3.25).abs() < 1e-9);
    }

    #[test]
    fn runtime_is_indefinite_without_discharge() {
        assert_eq!(project_runtime(65.0, 0.0), None);
    }

    #[test]
    fn runtime_is_zero_once_reserve_is_reached() {
        assert_eq!(project_runtime(0.0, 0.0), Some(0.0));
        assert_eq!(project_runtime(0.0, 20.0), Some(0.0));
    }

    #[test]
    fn estimate_math_derives_discharge_from_load() {
        assert!(
            (estimate_discharge_a(Some(2000.0), Some(100.0), Some(50.0)).unwrap()
                - 38.0 / INVERTER_EFFICIENCY)
                .abs()
                < 1e-9
        );
        assert_eq!(estimate_discharge_a(Some(2000.0), Some(5000.0), Some(50.0)), Some(0.0));
        assert_eq!(estimate_discharge_a(Some(100.0), Some(0.0), None), None);
        assert_eq!(estimate_discharge_a(None, Some(0.0), Some(50.0)), None);
    }

    #[test]
    fn tolerance_holds_high_soc_with_small_shortfall() {
        let config = AutomationConfig {
            deficit_tolerance_hours: 2.0,
            high_soc_hold_percent: 85.0,
            ..AutomationConfig::default()
        };
        assert!(within_tolerance(Some(96.0), Some(7.6), 8.5, &config));
        assert!(!within_tolerance(Some(70.0), Some(7.6), 8.5, &config));
        assert!(!within_tolerance(Some(96.0), Some(5.0), 8.5, &config));
        assert!(within_tolerance(Some(96.0), None, 8.5, &config));
    }

    #[test]
    fn shortfall_math_is_well_defined() {
        assert!((shortfall_hours(Some(7.6), 8.5) - 0.9).abs() < 1e-9);
        assert_eq!(shortfall_hours(None, 8.5), 0.0);
        assert_eq!(shortfall_hours(Some(9.0), 8.5), 0.0);
    }

    #[test]
    fn covers_treats_indefinite_runtime_as_covering() {
        assert!(covers(None, 8.0));
        assert!(covers(Some(9.0), 8.0));
        assert!(!covers(Some(7.9), 8.0));
    }

    #[test]
    fn day_rule_switches_to_solar_only_when_in_sbg() {
        let config = AutomationConfig {
            day_pv_threshold_watts: 300.0,
            day_discharge_threshold_a: 1.0,
            ..AutomationConfig::default()
        };
        assert!(should_switch_to_solar(Some(true), None, Some(SBG_MODE), Some(100.0), 5.0, &config));
        assert!(!should_switch_to_solar(Some(true), None, Some(0), Some(100.0), 5.0, &config), "no-op when already Solar");
        assert!(!should_switch_to_solar(Some(true), None, Some(SBG_MODE), Some(500.0), 5.0, &config), "PV too high");
        assert!(!should_switch_to_solar(Some(true), None, Some(SBG_MODE), Some(100.0), 0.5, &config), "discharge below threshold");
    }

    #[test]
    fn day_rule_respects_cooldown_and_grid_state() {
        let config = AutomationConfig {
            day_pv_threshold_watts: 300.0,
            day_discharge_threshold_a: 1.0,
            ..AutomationConfig::default()
        };
        assert!(!should_switch_to_solar(Some(false), None, Some(SBG_MODE), Some(100.0), 5.0, &config), "grid off");
        assert!(!should_switch_to_solar(Some(true), Some(Instant::now()), Some(SBG_MODE), Some(100.0), 5.0, &config), "fresh write cooldown");
        assert!(should_switch_to_solar(
            Some(true),
            Some(Instant::now() - Duration::from_secs(601)),
            Some(SBG_MODE),
            Some(100.0),
            5.0,
            &config
        ));
    }

    #[test]
    fn backed_off_requires_margin_before_re_engaging() {
        let required = 8.0;
        let runtime = 8.5;
        assert!(covers(Some(runtime), required));
        assert!(!covers(Some(runtime), required + BACKOFF_EXTRA_MARGIN_HOURS), "no re-flip with only base coverage");
        assert!(covers(Some(runtime + BACKOFF_EXTRA_MARGIN_HOURS), required + BACKOFF_EXTRA_MARGIN_HOURS));
    }

    #[test]
    fn resolver_prefers_connected_bms() {
        let config = AutomationConfig {
            reserve_soc_percent: 15.0,
            ..AutomationConfig::default()
        };
        let bms = bms_with(77, -18.5, 77.0, 100.0);
        let telemetry = resolve_telemetry(Some(&bms), &snapshot_with(50.0, 5.0, 100.0, 0), &config).unwrap();
        assert_eq!(telemetry.source, "bms");
        assert_eq!(telemetry.soc, 77.0);
        assert!((telemetry.discharge_a - 18.5).abs() < 1e-9);
        assert!((telemetry.usable_capacity_ah - 62.0).abs() < 1e-9);
    }

    #[test]
    fn resolver_reports_zero_discharge_when_bms_is_not_discharging() {
        let config = AutomationConfig::default();
        let bms = bms_with(90, 6.0, 90.0, 100.0);
        let telemetry = resolve_telemetry(Some(&bms), &snapshot_with(50.0, 5.0, 100.0, 0), &config).unwrap();
        assert_eq!(telemetry.source, "bms");
        assert_eq!(telemetry.discharge_a, 0.0);
    }

    #[test]
    fn resolver_falls_back_to_inverter() {
        let config = AutomationConfig {
            reserve_soc_percent: 15.0,
            capacity_ah: 100.0,
            ..AutomationConfig::default()
        };
        let telemetry = resolve_telemetry(None, &snapshot_with(80.0, 20.0, 100.0, 0), &config)
            .expect("inverter snapshot has SOC");
        assert_eq!(telemetry.source, "inverter");
        assert_eq!(telemetry.soc, 80.0);
        assert_eq!(telemetry.discharge_a, 20.0);
        assert!((telemetry.usable_capacity_ah - 65.0).abs() < 1e-9);
    }

    #[test]
    fn resolver_returns_none_without_any_soc() {
        let snapshot = InverterSnapshot {
            device_id: "test".into(),
            fields: serde_json::Map::new(),
            groups: vec![],
            firing_alarms: vec![],
            settings: None,
            pv_panel_flow: None,
            grid_flow: None,
            load_flow: None,
        };
        assert!(resolve_telemetry(None, &snapshot, &AutomationConfig::default()).is_none());
    }

    #[test]
    fn resolver_never_mixes_sources_within_one_reading() {
        let config = AutomationConfig::default();
        let bms = bms_with(60, -10.0, 60.0, 100.0);
        let telemetry = resolve_telemetry(Some(&bms), &snapshot_with(99.0, 1.0, 200.0, 0), &config).unwrap();
        assert_eq!(telemetry.soc, 60.0);
        assert_eq!(telemetry.discharge_a, 10.0);
        assert_ne!(telemetry.soc, 99.0);
    }
}