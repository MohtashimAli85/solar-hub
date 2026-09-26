use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Timelike};
use tauri::AppHandle;

use crate::automation::agent::{
    self, AgentError, AgentRunner, DecideInput, MorningInput, SunInfo, SunInput, VerifyInput,
};
use crate::automation::config::AutomationConfig;
use crate::automation::guardrails;
use crate::automation::telemetry::{estimate_discharge_a, EngineReading};
use crate::automation::{AutomationState, Phase};
use crate::battery::BatteryState;
use crate::inverter::client::SolarClient;
use crate::inverter::types::output_mode_name;
use crate::notifications::{NotificationSink, Notifier, Reading as NotifyReading};
use crate::storage::AppSettings;

const SBG_MODE: u32 = 1;
const SOLAR_MODE: u32 = 0;
const DECIDE_COOLDOWN: Duration = Duration::from_secs(3600);
const VERIFY_SETTLE: Duration = Duration::from_secs(150);
const VERIFY_SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
const VERIFY_SAMPLES_NEEDED: usize = 5;
const FAST_TICK: Duration = Duration::from_secs(60);

pub fn spawn(state: AutomationState, client: SolarClient, battery: BatteryState, notifier: Notifier, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let agent = agent::NodeAgent::new();
        run_loop(state, client, battery, notifier, app, &agent).await;
    });
}

async fn run_loop(
    state: AutomationState,
    client: SolarClient,
    battery: BatteryState,
    notifier: Notifier,
    app: AppHandle,
    agent: &dyn AgentRunner,
) {
    let mut memory = EngineMemory::default();
    loop {
        let config = state.config().await;
        let settings = crate::storage::load_app_settings(&app);
        let sleep_for = tick(&state, &client, &battery, &notifier, &config, &settings, agent, &mut memory).await;
        publish_status(&state, &app).await;
        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            _ = state.wake.notified() => {}
        }
    }
}

async fn publish_status(state: &AutomationState, app: &AppHandle) {
    let status = state.status().await;
    crate::events::emit(app, crate::events::AUTOMATION_STATUS, status);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Window {
    Night,
    Morning,
    Day,
}

fn select_window(
    now: DateTime<Local>,
    sunrise_today: DateTime<Local>,
    sunset_today: DateTime<Local>,
    morning_window_hours: f64,
) -> Window {
    if now < sunrise_today || now >= sunset_today {
        return Window::Night;
    }
    let morning_end = sunrise_today + chrono::Duration::milliseconds((morning_window_hours * 3_600_000.0) as i64);
    if now < morning_end {
        Window::Morning
    } else {
        Window::Day
    }
}

fn required_hours(
    now: DateTime<Local>,
    sunrise_today: DateTime<Local>,
    sunrise_tomorrow: DateTime<Local>,
    sunrise_buffer_hours: f64,
) -> f64 {
    let next_sunrise = if now < sunrise_today { sunrise_today } else { sunrise_tomorrow };
    let hours = (next_sunrise - now).num_seconds() as f64 / 3600.0;
    hours.max(0.0) + sunrise_buffer_hours
}

fn expected_pv_watts(pv_array_watts: f64, altitude_deg: f64) -> Option<f64> {
    if pv_array_watts <= 0.0 {
        return None;
    }
    if altitude_deg <= 0.0 {
        return Some(0.0);
    }
    Some(pv_array_watts * altitude_deg.to_radians().sin())
}

fn parse_local(iso: &str) -> Option<DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn normal_interval(config: &AutomationConfig) -> Duration {
    Duration::from_secs(config.check_interval_minutes.saturating_mul(60).max(60))
}

fn capitalize(body: &str) -> String {
    let mut chars = body.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

async fn notify_action(state: &AutomationState, notifier: &dyn NotificationSink, dry_run: bool, body: &str) {
    let message = if dry_run {
        format!("[Dry run] Would {body}")
    } else {
        capitalize(body)
    };
    state.notify_event(message.clone()).await;
    notifier.send("Automation", &message);
}

async fn notify_info(state: &AutomationState, notifier: &dyn NotificationSink, dry_run: bool, body: &str) {
    let message = if dry_run { format!("[Dry run] {body}") } else { body.to_string() };
    state.notify_event(message.clone()).await;
    notifier.send("Automation", &message);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NightPhase {
    Deciding,
    Verifying,
    Holding,
    Paused,
}

struct NightMemory {
    phase: NightPhase,
    previous_mode: Option<u32>,
    last_write_at: Option<Instant>,
    last_written_mode: Option<u32>,
    last_decide_at: Option<Instant>,
    decision_reason: String,
    verify_started_at: Option<Instant>,
    verify_samples: Vec<f64>,
    engagements: u32,
    ai_failure_notified: bool,
}

impl Default for NightMemory {
    fn default() -> Self {
        Self {
            phase: NightPhase::Deciding,
            previous_mode: None,
            last_write_at: None,
            last_written_mode: None,
            last_decide_at: None,
            decision_reason: String::new(),
            verify_started_at: None,
            verify_samples: Vec::new(),
            engagements: 0,
            ai_failure_notified: false,
        }
    }
}

#[derive(Default)]
struct MorningMemory {
    last_write_at: Option<Instant>,
    last_written_mode: Option<u32>,
    paused: bool,
    ai_failure_notified: bool,
}

struct SunCache {
    date: chrono::NaiveDate,
    latitude: f64,
    longitude: f64,
    info: SunInfo,
}

#[derive(Default)]
pub struct EngineMemory {
    window: Option<Window>,
    night: NightMemory,
    morning: MorningMemory,
    sun_cache: Option<SunCache>,
    blocked_notified: Option<String>,
}

fn on_window_change(memory: &mut EngineMemory, window: Window) {
    match window {
        Window::Night => memory.night = NightMemory::default(),
        Window::Morning => memory.morning = MorningMemory::default(),
        Window::Day => memory.night = NightMemory::default(),
    }
}

async fn ensure_sun(memory: &mut EngineMemory, agent: &dyn AgentRunner, latitude: f64, longitude: f64) -> Result<SunInfo, AgentError> {
    let today = chrono::Local::now().date_naive();
    if let Some(cache) = &memory.sun_cache {
        if cache.date == today && (cache.latitude - latitude).abs() < 1e-6 && (cache.longitude - longitude).abs() < 1e-6 {
            return Ok(cache.info.clone());
        }
    }
    let info = agent.run_sun(SunInput { latitude, longitude }).await?;
    memory.sun_cache = Some(SunCache {
        date: today,
        latitude,
        longitude,
        info: info.clone(),
    });
    Ok(info)
}

async fn enter_blocked(state: &AutomationState, notifier: &dyn NotificationSink, memory: &mut EngineMemory, reason: &str) -> Duration {
    state
        .set_status(|status| {
            status.phase = Phase::Blocked;
            status.blocked_reason = Some(reason.to_string());
        })
        .await;
    if memory.blocked_notified.as_deref() != Some(reason) {
        memory.blocked_notified = Some(reason.to_string());
        notify_info(state, notifier, false, reason).await;
    }
    Duration::from_secs(60)
}

#[allow(clippy::too_many_arguments)]
async fn tick(
    state: &AutomationState,
    client: &SolarClient,
    battery: &BatteryState,
    notifier: &Notifier,
    config: &AutomationConfig,
    settings: &AppSettings,
    agent: &dyn AgentRunner,
    memory: &mut EngineMemory,
) -> Duration {
    let snapshot = match client.read_inverter_snapshot(None).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!("automation could not read inverter snapshot: {error}");
            return normal_interval(config);
        }
    };
    let bms = if battery.connection_status().await.connected {
        battery.latest_snapshot().await
    } else {
        None
    };
    let reading = EngineReading::from_snapshot(&snapshot, bms.as_ref(), config).await;

    notifier
        .observe(NotifyReading {
            grid_on: reading.grid_on,
            soc: (reading.source == "inverter").then_some(reading.soc).flatten(),
            current_a: (reading.source == "inverter").then_some(reading.charge_a.or(Some(-reading.discharge_a))).flatten(),
            ..NotifyReading::default()
        })
        .await;

    state.set_status(|status| status.mode_name = reading.mode_name()).await;

    if !config.enabled {
        state
            .set_status(|status| {
                status.phase = Phase::Idle;
                status.blocked_reason = None;
            })
            .await;
        *memory = EngineMemory::default();
        return normal_interval(config);
    }

    let (Some(latitude), Some(longitude)) = (settings.latitude, settings.longitude) else {
        return enter_blocked(state, notifier, memory, "Set location above").await;
    };
    if crate::storage::get_gemini_api_key().ok().flatten().is_none() {
        return enter_blocked(state, notifier, memory, "Set a Gemini API key above").await;
    }

    let sun = match ensure_sun(memory, agent, latitude, longitude).await {
        Ok(sun) => sun,
        Err(AgentError::NodeNotFound) => {
            return enter_blocked(state, notifier, memory, "Node was not found — check your PATH").await;
        }
        Err(error) => {
            return enter_blocked(state, notifier, memory, &format!("Could not read sun info: {error}")).await;
        }
    };
    memory.blocked_notified = None;

    let Some(sunrise_today) = parse_local(&sun.today.sunrise) else {
        return normal_interval(config);
    };
    let Some(sunset_today) = parse_local(&sun.today.sunset) else {
        return normal_interval(config);
    };
    let Some(sunrise_tomorrow) = parse_local(&sun.tomorrow.sunrise) else {
        return normal_interval(config);
    };

    state
        .set_status(|status| {
            status.sunrise = Some(sunrise_today.to_rfc3339());
            status.sunset = Some(sunset_today.to_rfc3339());
        })
        .await;

    let now = chrono::Local::now();
    let window = select_window(now, sunrise_today, sunset_today, config.morning_window_hours);
    if memory.window != Some(window) {
        on_window_change(memory, window);
    }
    memory.window = Some(window);

    match window {
        Window::Night => {
            let required = required_hours(now, sunrise_today, sunrise_tomorrow, config.sunrise_buffer_hours);
            night_tick(state, client, notifier, config, agent, memory, &reading, required).await
        }
        Window::Morning => {
            let minutes_since_sunrise = (now - sunrise_today).num_seconds() as f64 / 60.0;
            morning_tick(state, client, notifier, config, agent, memory, &reading, latitude, longitude, minutes_since_sunrise).await
        }
        Window::Day => {
            state.set_status(|status| status.phase = Phase::Idle).await;
            normal_interval(config)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn night_tick(
    state: &AutomationState,
    client: &SolarClient,
    notifier: &dyn NotificationSink,
    config: &AutomationConfig,
    agent: &dyn AgentRunner,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    required_hours: f64,
) -> Duration {
    if guardrails::below_floor(reading.soc, config.min_soc_percent) {
        if memory.night.phase != NightPhase::Paused {
            revert_night(client, config, &mut memory.night).await;
            memory.night.phase = NightPhase::Paused;
            notify_info(state, notifier, config.dry_run, "Battery below floor — automation paused for the rest of the night").await;
        }
        state.set_status(|status| status.phase = Phase::Paused).await;
        return FAST_TICK;
    }

    if guardrails::user_took_over(reading.mode, memory.night.last_written_mode, config.dry_run) {
        if memory.night.phase != NightPhase::Paused {
            memory.night.phase = NightPhase::Paused;
            notify_info(state, notifier, config.dry_run, "You changed the output mode — automation paused for the rest of the night").await;
        }
        state.set_status(|status| status.phase = Phase::Paused).await;
        return normal_interval(config);
    }

    match memory.night.phase {
        NightPhase::Deciding => night_deciding(state, client, notifier, config, agent, memory, reading, required_hours).await,
        NightPhase::Verifying => night_verifying(state, client, notifier, config, agent, memory, reading, required_hours).await,
        NightPhase::Holding => {
            state.set_status(|status| status.phase = Phase::NightHolding).await;
            normal_interval(config)
        }
        NightPhase::Paused => {
            state.set_status(|status| status.phase = Phase::Paused).await;
            normal_interval(config)
        }
    }
}

async fn revert_night(client: &SolarClient, config: &AutomationConfig, night: &mut NightMemory) {
    if let Some(previous) = night.previous_mode.take() {
        if !config.dry_run {
            if let Err(error) = client.write_output_priority(None, previous.to_string()).await {
                tracing::warn!("automation could not restore previous mode: {error}");
                return;
            }
            night.last_write_at = Some(Instant::now());
            night.last_written_mode = Some(previous);
        }
    }
}

fn write_gate(grid_on: Option<bool>, last_write_at: Option<Instant>) -> bool {
    guardrails::can_write(grid_on, false, last_write_at, Instant::now())
}

#[allow(clippy::too_many_arguments)]
async fn night_deciding(
    state: &AutomationState,
    client: &SolarClient,
    notifier: &dyn NotificationSink,
    config: &AutomationConfig,
    agent: &dyn AgentRunner,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    required_hours: f64,
) -> Duration {
    state.set_status(|status| status.phase = Phase::NightDeciding).await;

    if reading.mode == Some(SBG_MODE) {
        memory.night.previous_mode = Some(SOLAR_MODE);
        memory.night.decision_reason = "already on SBG at the start of the window".into();
        enter_verifying(state, &mut memory.night).await;
        return FAST_TICK;
    }

    let due = memory.night.last_decide_at.is_none_or(|at| at.elapsed() >= DECIDE_COOLDOWN);
    if !due {
        return normal_interval(config);
    }
    memory.night.last_decide_at = Some(Instant::now());

    let discharge_a = estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v).unwrap_or(reading.discharge_a);
    let input = DecideInput {
        soc: reading.soc,
        discharge_a,
        usable_capacity_ah: reading.usable_capacity_ah,
        pv_w: reading.pv_w,
        load_w: reading.load_w,
        mode: reading.mode_name(),
        hour: chrono::Local::now().hour(),
        required_hours,
    };
    let decision = match agent.run_decide(input).await {
        Ok(decision) => decision,
        Err(error) => return ai_failure(state, notifier, config, &mut memory.night.ai_failure_notified, error, "Solar").await,
    };
    memory.night.ai_failure_notified = false;

    if decision.mode != "sbg" {
        return normal_interval(config);
    }
    if guardrails::engagement_cap_reached(memory.night.engagements) {
        memory.night.phase = NightPhase::Paused;
        notify_info(state, notifier, config.dry_run, "Reached tonight's SBG engagement limit — staying on Solar").await;
        state.set_status(|status| status.phase = Phase::Paused).await;
        return normal_interval(config);
    }
    if !write_gate(reading.grid_on, memory.night.last_write_at) {
        return normal_interval(config);
    }

    memory.night.previous_mode = reading.mode.or(Some(SOLAR_MODE));
    memory.night.decision_reason = decision.reason.clone();
    memory.night.engagements += 1;

    if !config.dry_run {
        if let Err(error) = client.write_output_priority(None, "sbg".into()).await {
            tracing::warn!("automation could not write sbg: {error}");
            return normal_interval(config);
        }
        memory.night.last_write_at = Some(Instant::now());
        memory.night.last_written_mode = Some(SBG_MODE);
    }

    notify_action(state, notifier, config.dry_run, &format!("switch to SBG — {}", decision.reason)).await;
    enter_verifying(state, &mut memory.night).await;
    FAST_TICK
}

async fn enter_verifying(state: &AutomationState, night: &mut NightMemory) {
    night.phase = NightPhase::Verifying;
    night.verify_started_at = Some(Instant::now());
    night.verify_samples.clear();
    state.set_status(|status| status.phase = Phase::NightVerifying).await;
}

#[allow(clippy::too_many_arguments)]
async fn night_verifying(
    state: &AutomationState,
    client: &SolarClient,
    notifier: &dyn NotificationSink,
    config: &AutomationConfig,
    agent: &dyn AgentRunner,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    required_hours: f64,
) -> Duration {
    state.set_status(|status| status.phase = Phase::NightVerifying).await;

    let Some(started) = memory.night.verify_started_at else {
        memory.night.verify_started_at = Some(Instant::now());
        return FAST_TICK;
    };
    if started.elapsed() < VERIFY_SETTLE {
        return FAST_TICK;
    }

    let sample = if config.dry_run {
        estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v).unwrap_or(0.0)
    } else {
        reading.discharge_a
    };
    memory.night.verify_samples.push(sample);
    if memory.night.verify_samples.len() < VERIFY_SAMPLES_NEEDED {
        return VERIFY_SAMPLE_INTERVAL;
    }

    let average = memory.night.verify_samples.iter().sum::<f64>() / memory.night.verify_samples.len() as f64;
    let verify_input = VerifyInput {
        original_reason: memory.night.decision_reason.clone(),
        soc: reading.soc,
        usable_capacity_ah: reading.usable_capacity_ah,
        verified_discharge_a: average,
        required_hours,
    };
    let verification = match agent.run_verify(verify_input).await {
        Ok(verification) => verification,
        Err(error) => return ai_failure(state, notifier, config, &mut memory.night.ai_failure_notified, error, "SBG").await,
    };
    memory.night.ai_failure_notified = false;

    if verification.verified {
        memory.night.phase = NightPhase::Holding;
        notify_info(state, notifier, config.dry_run, &format!("Verified — {}", verification.reason)).await;
        state.set_status(|status| status.phase = Phase::NightHolding).await;
        return normal_interval(config);
    }

    let redecide_input = DecideInput {
        soc: reading.soc,
        discharge_a: average,
        usable_capacity_ah: reading.usable_capacity_ah,
        pv_w: reading.pv_w,
        load_w: reading.load_w,
        mode: reading.mode_name(),
        hour: chrono::Local::now().hour(),
        required_hours,
    };
    let decision = match agent.run_decide(redecide_input).await {
        Ok(decision) => decision,
        Err(error) => return ai_failure(state, notifier, config, &mut memory.night.ai_failure_notified, error, "SBG").await,
    };

    if decision.mode != "sbg" {
        revert_night(client, config, &mut memory.night).await;
        notify_action(state, notifier, config.dry_run, &format!("revert to Solar — {}", decision.reason)).await;
        memory.night.phase = NightPhase::Deciding;
        memory.night.last_decide_at = Some(Instant::now());
        state.set_status(|status| status.phase = Phase::NightDeciding).await;
        return normal_interval(config);
    }

    memory.night.decision_reason = decision.reason;
    enter_verifying(state, &mut memory.night).await;
    FAST_TICK
}

async fn ai_failure(
    state: &AutomationState,
    notifier: &dyn NotificationSink,
    config: &AutomationConfig,
    already_notified: &mut bool,
    error: AgentError,
    fallback_mode: &str,
) -> Duration {
    tracing::warn!("automation agent call failed: {error}");
    if !*already_notified {
        *already_notified = true;
        notify_info(state, notifier, config.dry_run, &format!("AI unavailable — keeping {fallback_mode}")).await;
    }
    normal_interval(config)
}

#[allow(clippy::too_many_arguments)]
async fn morning_tick(
    state: &AutomationState,
    client: &SolarClient,
    notifier: &dyn NotificationSink,
    config: &AutomationConfig,
    agent: &dyn AgentRunner,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    latitude: f64,
    longitude: f64,
    minutes_since_sunrise: f64,
) -> Duration {
    if memory.morning.paused {
        state.set_status(|status| status.phase = Phase::Paused).await;
        return normal_interval(config);
    }

    if guardrails::user_took_over(reading.mode, memory.morning.last_written_mode, config.dry_run) {
        memory.morning.paused = true;
        notify_info(state, notifier, config.dry_run, "You changed the output mode — automation paused until tonight").await;
        state.set_status(|status| status.phase = Phase::Paused).await;
        return normal_interval(config);
    }

    state.set_status(|status| status.phase = Phase::Morning).await;

    let altitude_deg = agent
        .run_sun(SunInput { latitude, longitude })
        .await
        .ok()
        .map(|info| info.altitude_deg);
    let expected_pv_w = altitude_deg.and_then(|deg| expected_pv_watts(config.pv_array_watts, deg));

    let input = MorningInput {
        soc: reading.soc,
        charge_a: reading.charge_a,
        charge_threshold_a: config.morning_charge_threshold_a,
        pv_w: reading.pv_w,
        expected_pv_w,
        load_w: reading.load_w,
        mode: reading.mode_name(),
        minutes_since_sunrise,
    };
    let decision = match agent.run_morning(input).await {
        Ok(decision) => decision,
        Err(error) => return ai_failure(state, notifier, config, &mut memory.morning.ai_failure_notified, error, "current mode").await,
    };
    memory.morning.ai_failure_notified = false;

    let target_mode = if decision.mode == "sbg" { SBG_MODE } else { SOLAR_MODE };
    if reading.mode == Some(target_mode) {
        return normal_interval(config);
    }
    if !write_gate(reading.grid_on, memory.morning.last_write_at) {
        return normal_interval(config);
    }

    if !config.dry_run {
        if let Err(error) = client.write_output_priority(None, target_mode.to_string()).await {
            tracing::warn!("automation could not write morning mode: {error}");
            return normal_interval(config);
        }
        memory.morning.last_write_at = Some(Instant::now());
        memory.morning.last_written_mode = Some(target_mode);
    }

    notify_action(
        state,
        notifier,
        config.dry_run,
        &format!("switch to {} — {}", output_mode_name(target_mode), decision.reason),
    )
    .await;
    normal_interval(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inverter::client::SolarCredentials;
    use chrono::TimeZone;
    use futures_util::future::BoxFuture;
    use std::sync::Mutex as StdMutex;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn window_selects_night_across_midnight() {
        let sunrise = local(2026, 1, 10, 6, 30);
        let sunset = local(2026, 1, 10, 18, 0);
        assert_eq!(select_window(local(2026, 1, 10, 23, 0), sunrise, sunset, 3.0), Window::Night);
        assert_eq!(select_window(local(2026, 1, 10, 5, 0), sunrise, sunset, 3.0), Window::Night);
        assert_eq!(select_window(local(2026, 1, 10, 19, 0), sunrise, sunset, 3.0), Window::Night);
    }

    #[test]
    fn window_selects_morning_then_day() {
        let sunrise = local(2026, 1, 10, 6, 30);
        let sunset = local(2026, 1, 10, 18, 0);
        assert_eq!(select_window(local(2026, 1, 10, 7, 0), sunrise, sunset, 3.0), Window::Morning);
        assert_eq!(select_window(local(2026, 1, 10, 10, 0), sunrise, sunset, 3.0), Window::Day);
    }

    #[test]
    fn required_hours_before_midnight_targets_todays_sunrise_tomorrow() {
        let now = local(2026, 1, 10, 21, 0);
        let sunrise_today = local(2026, 1, 10, 6, 30);
        let sunrise_tomorrow = local(2026, 1, 11, 6, 30);
        let hours = required_hours(now, sunrise_today, sunrise_tomorrow, 1.0);
        assert!((hours - (9.5 + 1.0)).abs() < 1e-9);
    }

    #[test]
    fn required_hours_after_midnight_targets_todays_sunrise() {
        let now = local(2026, 1, 11, 2, 0);
        let sunrise_today = local(2026, 1, 11, 6, 30);
        let sunrise_tomorrow = local(2026, 1, 12, 6, 30);
        let hours = required_hours(now, sunrise_today, sunrise_tomorrow, 1.0);
        assert!((hours - (4.5 + 1.0)).abs() < 1e-9);
    }

    #[test]
    fn expected_pv_is_none_without_array_size() {
        assert_eq!(expected_pv_watts(0.0, 45.0), None);
    }

    #[test]
    fn expected_pv_is_zero_below_horizon() {
        assert_eq!(expected_pv_watts(1000.0, -5.0), Some(0.0));
    }

    struct FakeAgent {
        decide: StdMutex<Vec<agent::Decision>>,
        verify: StdMutex<Vec<agent::Verification>>,
    }

    impl FakeAgent {
        fn new(decide: Vec<agent::Decision>, verify: Vec<agent::Verification>) -> Self {
            Self {
                decide: StdMutex::new(decide),
                verify: StdMutex::new(verify),
            }
        }
    }

    impl AgentRunner for FakeAgent {
        fn run_sun(&self, _input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>> {
            Box::pin(async {
                Ok(SunInfo {
                    today: agent::SunToday {
                        sunrise: "2026-01-10T00:30:00Z".into(),
                        sunset: "2026-01-10T12:00:00Z".into(),
                    },
                    tomorrow: agent::SunTomorrow {
                        sunrise: "2026-01-11T00:30:00Z".into(),
                    },
                    altitude_deg: 10.0,
                })
            })
        }

        fn run_decide(&self, _input: DecideInput) -> BoxFuture<'_, Result<agent::Decision, AgentError>> {
            let decision = self.decide.lock().unwrap().remove(0);
            Box::pin(async move { Ok(decision) })
        }

        fn run_verify(&self, _input: VerifyInput) -> BoxFuture<'_, Result<agent::Verification, AgentError>> {
            let verification = self.verify.lock().unwrap().remove(0);
            Box::pin(async move { Ok(verification) })
        }

        fn run_morning(&self, _input: MorningInput) -> BoxFuture<'_, Result<agent::Decision, AgentError>> {
            let decision = self.decide.lock().unwrap().remove(0);
            Box::pin(async move { Ok(decision) })
        }
    }

    #[derive(Default)]
    struct FakeNotifier {
        sent: StdMutex<Vec<(String, String)>>,
    }

    impl NotificationSink for FakeNotifier {
        fn send(&self, title: &str, body: &str) {
            self.sent.lock().unwrap().push((title.to_string(), body.to_string()));
        }
    }

    fn test_client() -> SolarClient {
        SolarClient::new(SolarCredentials::default())
    }

    fn reading_with(soc: f64, mode: u32, grid_on: bool) -> EngineReading {
        EngineReading {
            soc: Some(soc),
            discharge_a: 5.0,
            charge_a: None,
            usable_capacity_ah: 50.0,
            pv_w: Some(0.0),
            load_w: Some(300.0),
            batt_v: Some(50.0),
            grid_on: Some(grid_on),
            mode: Some(mode),
            smart_load: None,
            source: "bms",
        }
    }

    fn dry_run_config() -> AutomationConfig {
        AutomationConfig {
            enabled: true,
            dry_run: true,
            ..AutomationConfig::default()
        }
    }

    fn decision(mode: &str, reason: &str) -> agent::Decision {
        agent::Decision {
            mode: mode.into(),
            confidence: 0.9,
            reason: reason.into(),
        }
    }

    fn verification(verified: bool, reason: &str) -> agent::Verification {
        agent::Verification {
            verified,
            confidence: 0.9,
            reason: reason.into(),
        }
    }

    #[tokio::test]
    async fn deciding_switches_to_verifying_on_sbg_decision() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![decision("sbg", "battery covers the night")], vec![]);
        let config = dry_run_config();
        let mut memory = EngineMemory::default();
        let reading = reading_with(80.0, SOLAR_MODE, true);

        let delay = night_deciding(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Verifying);
        assert_eq!(delay, FAST_TICK);
        assert_eq!(state.status().await.phase, Phase::NightVerifying);
    }

    #[tokio::test]
    async fn deciding_stays_on_solar_when_ai_says_solar() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![decision("solar", "battery covers little")], vec![]);
        let config = dry_run_config();
        let mut memory = EngineMemory::default();
        let reading = reading_with(80.0, SOLAR_MODE, true);

        night_deciding(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Deciding);
    }

    #[tokio::test]
    async fn verifying_confirms_and_holds() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![], vec![verification(true, "runtime covers the gap")]);
        let config = dry_run_config();
        let mut memory = EngineMemory::default();
        memory.night.phase = NightPhase::Verifying;
        memory.night.verify_started_at = Some(Instant::now() - VERIFY_SETTLE - Duration::from_secs(1));
        memory.night.verify_samples = vec![10.0, 10.0, 10.0, 10.0];
        let reading = reading_with(80.0, SBG_MODE, true);

        night_verifying(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Holding);
        assert_eq!(state.status().await.phase, Phase::NightHolding);
    }

    #[tokio::test]
    async fn verifying_fails_then_redecides_to_solar_and_reverts() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![decision("solar", "draining too fast")], vec![verification(false, "won't last")]);
        let config = dry_run_config();
        let mut memory = EngineMemory::default();
        memory.night.phase = NightPhase::Verifying;
        memory.night.previous_mode = Some(SOLAR_MODE);
        memory.night.verify_started_at = Some(Instant::now() - VERIFY_SETTLE - Duration::from_secs(1));
        memory.night.verify_samples = vec![30.0, 30.0, 30.0, 30.0];
        let reading = reading_with(80.0, SBG_MODE, true);

        night_verifying(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Deciding);
        assert_eq!(state.status().await.phase, Phase::NightDeciding);
    }

    #[tokio::test]
    async fn floor_breach_pauses_for_the_rest_of_the_night() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![], vec![]);
        let mut config = dry_run_config();
        config.min_soc_percent = 20.0;
        let mut memory = EngineMemory::default();
        let reading = reading_with(15.0, SOLAR_MODE, true);

        night_tick(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Paused);
    }

    #[tokio::test]
    async fn user_takeover_pauses_for_the_rest_of_the_night() {
        let state = AutomationState::new(AutomationConfig {
            enabled: true,
            dry_run: false,
            ..AutomationConfig::default()
        });
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![], vec![]);
        let config = AutomationConfig {
            enabled: true,
            dry_run: false,
            ..AutomationConfig::default()
        };
        let mut memory = EngineMemory::default();
        memory.night.last_written_mode = Some(SBG_MODE);
        let reading = reading_with(80.0, SOLAR_MODE, true);

        night_tick(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Paused);
    }

    #[tokio::test]
    async fn engagement_cap_pauses_deciding() {
        let state = AutomationState::new(dry_run_config());
        let notifier = FakeNotifier::default();
        let client = test_client();
        let agent = FakeAgent::new(vec![decision("sbg", "covers the gap")], vec![]);
        let config = dry_run_config();
        let mut memory = EngineMemory::default();
        memory.night.engagements = guardrails::MAX_ENGAGEMENTS_PER_NIGHT;
        let reading = reading_with(80.0, SOLAR_MODE, true);

        night_deciding(&state, &client, &notifier, &config, &agent, &mut memory, &reading, 8.0).await;

        assert_eq!(memory.night.phase, NightPhase::Paused);
    }
}
