use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use tauri::AppHandle;

use crate::automation::agent::{
    self, AgentError, AgentRunner, DayInput, HourLoadRow, HourOutlookRow, NextDayRow, NightInput, OutageRow,
    PreviousPlan, RecentDayRow, SmartLoadBrief, SocRow, SunInfo, SunInput, WeatherBrief,
};
use crate::automation::config::AutomationConfig;
use crate::automation::guardrails;
use crate::automation::history::{self, DecisionRow, NightProfile, Outage, Sample};
use crate::automation::insights::{AutomationInsights, RecordsInfo, RoutineInsight, SmartLoadInsight};
use crate::automation::telemetry::{
    estimate_discharge_a, first_at_or_below, project_day, soc_per_hour_at, soc_trajectory, DayProjection,
    DayProjectionInput, EngineReading, SocPoint,
};
use crate::automation::weather::{self, Forecast, SummaryInput, WeatherSummary};
use crate::automation::{AutomationState, Phase};
use crate::battery::BatteryState;
use crate::inverter::client::SolarClient;
use crate::inverter::types::output_mode_name;
use crate::notifications::{NotificationSink, Notifier, Reading as NotifyReading};
use crate::storage::AppSettings;

const SBG_MODE: u32 = 1;
const SOLAR_MODE: u32 = 0;
const VERIFY_SETTLE: chrono::Duration = chrono::Duration::seconds(150);
const VERIFY_SAMPLE_INTERVAL: Duration = Duration::from_secs(60);
const VERIFY_SAMPLES_NEEDED: usize = 5;
const FAST_TICK: Duration = Duration::from_secs(60);
const NEAR_RESERVE_TICK: Duration = Duration::from_secs(300);
const NEAR_RESERVE_MARGIN: f64 = 5.0;
const MAX_AI_FAILURES: u32 = 3;
const AI_RETRY_MINUTES: i64 = 15;
const SAMPLE_EVERY: Duration = Duration::from_secs(290);
const WEATHER_REFRESH: Duration = Duration::from_secs(30 * 60);
const WEATHER_RETRY: Duration = Duration::from_secs(10 * 60);
const WEATHER_MAX_AGE: Duration = Duration::from_secs(6 * 3600);
const MIN_RECHECK_MINUTES: f64 = 15.0;
const MAX_RECHECK_MINUTES: f64 = 120.0;
const MAX_RESERVE_SOC: f64 = 95.0;
const HISTORY_NIGHTS: usize = 7;
const RECENT_DAYS: i64 = 7;
const NOMINAL_PACK_VOLTS: f64 = 51.2;
const INVERTER_EFFICIENCY: f64 = 0.9;
const SUMMER_MONTHS: std::ops::RangeInclusive<u32> = 4..=8;
const SMART_LOAD_LATEST_HOUR: u32 = 23;
const DAY_DRAIN_A: f64 = 1.0;
const DRAIN_CHECKS_TO_SWITCH: u32 = 2;
const SUN_GONE_PV_W: f64 = 50.0;
const SUN_GONE_BEFORE_SUNSET_MINUTES: i64 = 60;
const AFTER_DRAIN_RECHECK_MINUTES: i64 = 60;
const BOOST_MINUTES: i64 = 3;
const BOOST_GAP_MINUTES: i64 = 5;
const MAX_BOOSTS_PER_NIGHT: u32 = 4;
const BOOST_RESERVE_MARGIN: f64 = 2.0;
const BOOST_POLL: Duration = Duration::from_secs(30);

pub fn spawn(state: AutomationState, client: SolarClient, battery: BatteryState, notifier: Notifier, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let agent = agent::NodeAgent::new();
        let weather_http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("weather http client");
        let mut memory = EngineMemory::default();
        loop {
            let config = state.config().await;
            let settings = crate::storage::load_app_settings(&app);
            let services = Services {
                client: &client,
                battery: &battery,
                notifier: &notifier,
                agent: &agent,
                weather_http: &weather_http,
            };
            let sleep_for = tick(&state, &services, &config, &settings, &mut memory).await;
            let status = state.status().await;
            crate::events::emit(&app, crate::events::AUTOMATION_STATUS, status);
            tokio::select! {
                _ = tokio::time::sleep(sleep_for) => {}
                _ = state.wake.notified() => {}
            }
        }
    });
}

struct Services<'a> {
    client: &'a SolarClient,
    battery: &'a BatteryState,
    notifier: &'a Notifier,
    agent: &'a dyn AgentRunner,
    weather_http: &'a reqwest::Client,
}

/// The collaborators a decision needs; split out so tests can run the state
/// machine without a network or an `AppHandle`.
struct Engine<'a> {
    state: &'a AutomationState,
    client: &'a SolarClient,
    notifier: &'a dyn NotificationSink,
    agent: &'a dyn AgentRunner,
    config: &'a AutomationConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Window {
    Day,
    Night,
}

impl Window {
    fn label(self) -> &'static str {
        match self {
            Window::Day => "day",
            Window::Night => "night",
        }
    }
}

struct Clock {
    now: NaiveDateTime,
    sunrise_today: NaiveDateTime,
    sunset_today: NaiveDateTime,
    sunrise_tomorrow: NaiveDateTime,
    night_start_hour: u32,
}

impl Clock {
    /// Pins sunrise/sunset to today's and tomorrow's calendar dates. The sun
    /// times only move a minute or so a day, but a stale day (yesterday's
    /// sunrise just after midnight) would make the whole night look like day.
    fn for_today(
        now: NaiveDateTime,
        sunrise: NaiveDateTime,
        sunset: NaiveDateTime,
        next_sunrise: NaiveDateTime,
        night_start_hour: u32,
    ) -> Self {
        let today = now.date();
        let tomorrow = today.succ_opt().unwrap_or(today);
        Self {
            now,
            sunrise_today: today.and_time(sunrise.time()),
            sunset_today: today.and_time(sunset.time()),
            sunrise_tomorrow: tomorrow.and_time(next_sunrise.time()),
            night_start_hour,
        }
    }

    fn night_start_on(&self, date: NaiveDate) -> NaiveDateTime {
        date.and_hms_opt(self.night_start_hour, 0, 0).unwrap_or(self.now)
    }

    /// Day runs from sunrise to the configured night start (21:00 by default),
    /// so the evening after sunset stays on grid.
    fn window(&self) -> Window {
        if self.now >= self.sunrise_today && self.now < self.night_start_on(self.now.date()) {
            Window::Day
        } else {
            Window::Night
        }
    }

    fn next_sunrise(&self) -> NaiveDateTime {
        if self.now < self.sunrise_today {
            self.sunrise_today
        } else {
            self.sunrise_tomorrow
        }
    }

    /// The night that is running now, or the one coming up this evening.
    fn night(&self) -> NaiveDate {
        if self.now < self.sunrise_today {
            self.now.date().pred_opt().unwrap_or(self.now.date())
        } else {
            self.now.date()
        }
    }

    fn night_start(&self) -> NaiveDateTime {
        self.night_start_on(self.night())
    }

    fn night_on(&self, hour: u32) -> NaiveDateTime {
        self.night().and_hms_opt(hour, 0, 0).unwrap_or(self.now)
    }

    fn next_solar_day(&self) -> NaiveDate {
        if self.now < self.sunrise_today {
            self.now.date()
        } else {
            self.now.date().succ_opt().unwrap_or(self.now.date())
        }
    }
}

struct Context {
    clock: Clock,
    latitude: f64,
    samples: Vec<Sample>,
    profile: NightProfile,
    day_profile: Vec<history::HourLoad>,
    outages: Vec<Outage>,
    forecast: Option<Forecast>,
    weather: Option<WeatherSummary>,
}

impl Context {
    fn build(clock: Clock, latitude: f64, samples: Vec<Sample>, forecast: Option<Forecast>, config: &AutomationConfig) -> Self {
        let profile = history::night_profile(&samples, clock.night(), HISTORY_NIGHTS);
        let day_profile = history::day_profile(&samples, clock.now.date(), RECENT_DAYS);
        let outages = history::outages(&samples, clock.now - chrono::Duration::days(RECENT_DAYS));
        let solar_days = history::daily_solar(&samples, clock.now.date(), RECENT_DAYS);
        let weather = forecast.as_ref().map(|forecast| {
            weather::summarize(
                forecast,
                SummaryInput {
                    now: clock.now,
                    next_solar_day: clock.next_solar_day(),
                    night: clock.night(),
                    sunset: clock.sunset_today,
                    pv_array_watts: config.pv_array_watts,
                    solar_history: &solar_days,
                },
            )
        });
        Self {
            clock,
            latitude,
            samples,
            profile,
            day_profile,
            outages,
            forecast,
            weather,
        }
    }
}

fn normal_interval(config: &AutomationConfig) -> Duration {
    Duration::from_secs(config.check_interval_minutes.saturating_mul(60).max(60))
}

fn until(now: NaiveDateTime, next: Option<NaiveDateTime>, cap: Duration) -> Duration {
    match next {
        Some(next) if next > now => (next - now).to_std().unwrap_or(cap).min(cap).max(Duration::from_secs(30)),
        Some(_) => Duration::from_secs(30),
        None => cap,
    }
}

fn parse_local(iso: &str) -> Option<NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.with_timezone(&Local).naive_local())
}

fn capitalize(body: &str) -> String {
    let mut chars = body.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn hhmm(at: NaiveDateTime) -> String {
    at.format("%H:%M").to_string()
}

fn mode_label(mode: Option<u32>) -> Option<String> {
    mode.map(output_mode_name)
}

impl Engine<'_> {
    async fn notify_action(&self, body: &str) {
        let message = if self.config.dry_run {
            format!("[Dry run] Would {body}")
        } else {
            capitalize(body)
        };
        self.state.notify_event(message.clone()).await;
        self.notifier.send("Automation", &message);
    }

    async fn notify_info(&self, body: &str) {
        let message = if self.config.dry_run { format!("[Dry run] {body}") } else { body.to_string() };
        self.state.notify_event(message.clone()).await;
        self.notifier.send("Automation", &message);
    }

    async fn set_phase(&self, phase: Phase) {
        self.state.set_status(|status| status.phase = phase).await;
    }

    async fn set_reason(&self, reason: &str) {
        let reason = reason.to_string();
        self.state.set_status(|status| status.ai_reason = Some(reason)).await;
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        at: NaiveDateTime,
        window: Window,
        mode: &str,
        reserve_soc: Option<f64>,
        recheck_minutes: Option<u32>,
        confidence: Option<f64>,
        applied: bool,
        reason: &str,
    ) {
        let row = DecisionRow {
            at,
            window: window.label().into(),
            mode: mode.into(),
            reserve_soc,
            recheck_minutes,
            confidence,
            dry_run: self.config.dry_run,
            applied,
            reason: reason.into(),
        };
        if let Err(error) = self.state.history.append_decision(&row) {
            tracing::warn!("could not record automation decision: {error}");
        }
    }

    /// Dry run never touches the inverter but reports success, so the engine
    /// follows the same path it would for real.
    async fn write_mode(&self, mode: u32) -> bool {
        if self.config.dry_run {
            return true;
        }
        match self.client.write_output_priority(None, mode.to_string()).await {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!("automation could not write output mode {mode}: {error}");
                false
            }
        }
    }

    async fn ai_failure(&self, already_notified: &mut bool, error: AgentError, holding: &str) {
        tracing::warn!("automation agent call failed: {error}");
        if !*already_notified {
            *already_notified = true;
            self.notify_info(&format!("AI unavailable — keeping {holding}")).await;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NightPhase {
    Deciding,
    Verifying,
    OnBattery,
    ReserveKept,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
struct ActivePlan {
    reserve_soc: f64,
    reason: String,
}

struct NightMemory {
    phase: NightPhase,
    /// The engine's own view of the output mode. In dry run it follows the
    /// writes the engine *would* have made, not the physical inverter.
    effective_mode: Option<u32>,
    sim_soc: Option<f64>,
    sim_updated_at: Option<NaiveDateTime>,
    plan: Option<ActivePlan>,
    next_check_at: Option<NaiveDateTime>,
    on_battery_since: Option<NaiveDateTime>,
    verify_started_at: Option<NaiveDateTime>,
    verify_samples: Vec<f64>,
    ai_failures: u32,
    ai_failure_notified: bool,
    engagements: u32,
    last_write_at: Option<Instant>,
    last_written_mode: Option<u32>,
    smart_load_on_at: Option<NaiveDateTime>,
    smart_load_done: bool,
    last_reserve_soc: Option<f64>,
    boost: Option<Boost>,
    boosts: u32,
    last_boost_end: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq)]
struct Boost {
    until: NaiveDateTime,
    smart_load_was_on: bool,
}

impl Default for NightMemory {
    fn default() -> Self {
        Self {
            phase: NightPhase::Deciding,
            effective_mode: None,
            sim_soc: None,
            sim_updated_at: None,
            plan: None,
            next_check_at: None,
            on_battery_since: None,
            verify_started_at: None,
            verify_samples: Vec::new(),
            ai_failures: 0,
            ai_failure_notified: false,
            engagements: 0,
            last_write_at: None,
            last_written_mode: None,
            smart_load_on_at: None,
            smart_load_done: false,
            last_reserve_soc: None,
            boost: None,
            boosts: 0,
            last_boost_end: None,
        }
    }
}

#[derive(Default)]
struct DayMemory {
    effective_mode: Option<u32>,
    paused: bool,
    next_check_at: Option<NaiveDateTime>,
    last_write_at: Option<Instant>,
    last_written_mode: Option<u32>,
    ai_failure_notified: bool,
    smart_load_done: bool,
    forced: bool,
    drain_checks: u32,
}

struct SunCache {
    date: NaiveDate,
    latitude: f64,
    longitude: f64,
    info: SunInfo,
}

struct WeatherCache {
    fetched_at: Instant,
    latitude: f64,
    longitude: f64,
    forecast: Forecast,
}

#[derive(Default)]
pub struct EngineMemory {
    window: Option<Window>,
    night: NightMemory,
    day: DayMemory,
    sun_cache: Option<SunCache>,
    weather: Option<WeatherCache>,
    weather_failed_at: Option<Instant>,
    blocked_notified: Option<String>,
    last_sample_at: Option<Instant>,
    last_mode: Option<u32>,
    dry_run: Option<bool>,
}

impl EngineMemory {
    fn reset_automation(&mut self) {
        self.window = None;
        self.night = NightMemory::default();
        self.day = DayMemory::default();
        self.blocked_notified = None;
    }
}

fn same_place(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

async fn ensure_sun(memory: &mut EngineMemory, agent: &dyn AgentRunner, latitude: f64, longitude: f64) -> Result<SunInfo, AgentError> {
    let today = Local::now().date_naive();
    if let Some(cache) = &memory.sun_cache {
        if cache.date == today && same_place((cache.latitude, cache.longitude), (latitude, longitude)) {
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

async fn refresh_weather(memory: &mut EngineMemory, http: &reqwest::Client, latitude: f64, longitude: f64) {
    let fresh = memory.weather.as_ref().is_some_and(|cache| {
        same_place((cache.latitude, cache.longitude), (latitude, longitude)) && cache.fetched_at.elapsed() < WEATHER_REFRESH
    });
    if fresh || memory.weather_failed_at.is_some_and(|at| at.elapsed() < WEATHER_RETRY) {
        return;
    }
    match weather::fetch(http, latitude, longitude).await {
        Ok(forecast) => {
            memory.weather = Some(WeatherCache {
                fetched_at: Instant::now(),
                latitude,
                longitude,
                forecast,
            });
            memory.weather_failed_at = None;
        }
        Err(error) => {
            tracing::warn!("weather forecast unavailable: {error}");
            memory.weather_failed_at = Some(Instant::now());
        }
    }
}

fn usable_forecast(memory: &EngineMemory, latitude: f64, longitude: f64) -> Option<Forecast> {
    memory
        .weather
        .as_ref()
        .filter(|cache| {
            same_place((cache.latitude, cache.longitude), (latitude, longitude)) && cache.fetched_at.elapsed() < WEATHER_MAX_AGE
        })
        .map(|cache| cache.forecast.clone())
}

fn record_sample(state: &AutomationState, memory: &mut EngineMemory, reading: &EngineReading, now: NaiveDateTime) {
    if memory.last_sample_at.is_some_and(|at| at.elapsed() < SAMPLE_EVERY) {
        return;
    }
    memory.last_sample_at = Some(Instant::now());
    let source = match (reading.source, reading.battery_a.is_some()) {
        ("bms_only", _) => "bms_only",
        (_, true) => "inverter+bms",
        _ => "inverter",
    };
    let sample = Sample {
        at: now,
        soc: reading.soc,
        load_w: reading.load_w,
        pv_w: reading.pv_w,
        battery_a: reading.battery_a,
        battery_v: reading.batt_v,
        grid_on: reading.grid_on(),
        grid_basis: reading.grid_basis().as_str().into(),
        mode: reading.mode_name(),
        source: source.into(),
    };
    if let Err(error) = state.history.append_sample(&sample) {
        tracing::warn!("could not record sample to {}: {error}", state.history.root().display());
    }
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
        state.notify_event(reason.to_string()).await;
        notifier.send("Automation", reason);
    }
    Duration::from_secs(60)
}

async fn tick(
    state: &AutomationState,
    services: &Services<'_>,
    config: &AutomationConfig,
    settings: &AppSettings,
    memory: &mut EngineMemory,
) -> Duration {
    let now = Local::now().naive_local().with_nanosecond(0).unwrap_or_else(|| Local::now().naive_local());
    let bms = if services.battery.connection_status().await.connected {
        services.battery.latest_snapshot().await
    } else {
        None
    };

    let reading = match services.client.read_inverter_snapshot(None).await {
        Ok(snapshot) => {
            memory.last_mode = snapshot.output_mode().or(memory.last_mode);
            EngineReading::from_snapshot(&snapshot, bms.as_ref(), config)
        }
        Err(error) => {
            tracing::warn!("automation could not read inverter snapshot: {error}");
            let Some(bms) = bms.as_ref() else {
                return normal_interval(config);
            };
            let reading = EngineReading::from_bms_only(bms, memory.last_mode, config);
            services.notifier.observe(NotifyReading { grid_on: reading.grid_on(), ..NotifyReading::default() }).await;
            record_sample(state, memory, &reading, now);
            state
                .set_status(|status| status.blocked_reason = Some("Inverter cloud unreachable — recording from the battery only".into()))
                .await;
            return normal_interval(config);
        }
    };

    let from_inverter = reading.source == "inverter";
    services
        .notifier
        .observe(NotifyReading {
            grid_on: reading.grid_on(),
            soc: from_inverter.then_some(reading.soc).flatten(),
            current_a: from_inverter.then_some(reading.charge_a.or(Some(-reading.discharge_a))).flatten(),
            ..NotifyReading::default()
        })
        .await;
    record_sample(state, memory, &reading, now);
    state
        .set_status(|status| {
            status.mode_name = reading.mode_name();
            status.blocked_reason = None;
        })
        .await;

    let engine = Engine {
        state,
        client: services.client,
        notifier: services.notifier,
        agent: services.agent,
        config,
    };

    let (Some(latitude), Some(longitude)) = (settings.latitude, settings.longitude) else {
        if config.enabled {
            return enter_blocked(state, services.notifier, memory, "Set your location in Settings").await;
        }
        return idle(&engine, memory).await;
    };

    let sun = match ensure_sun(memory, services.agent, latitude, longitude).await {
        Ok(sun) => sun,
        Err(error) if config.enabled => {
            let reason = match error {
                AgentError::NodeNotFound => "Node was not found — check your PATH".to_string(),
                other => format!("Could not read sun times: {other}"),
            };
            return enter_blocked(state, services.notifier, memory, &reason).await;
        }
        Err(_) => return idle(&engine, memory).await,
    };
    let (Some(sunrise_today), Some(sunset_today), Some(sunrise_tomorrow)) = (
        parse_local(&sun.today.sunrise),
        parse_local(&sun.today.sunset),
        parse_local(&sun.tomorrow.sunrise),
    ) else {
        return normal_interval(config);
    };
    state
        .set_status(|status| {
            status.sunrise = Some(sun.today.sunrise.clone());
            status.sunset = Some(sun.today.sunset.clone());
        })
        .await;

    refresh_weather(memory, services.weather_http, latitude, longitude).await;
    let clock = Clock::for_today(now, sunrise_today, sunset_today, sunrise_tomorrow, config.night_start_hour);
    let samples = state.history.recent_samples(now.date());
    let ctx = Context::build(clock, latitude, samples, usable_forecast(memory, latitude, longitude), config);

    // Dry run's simulated state (e.g. "already on battery") doesn't match the
    // real inverter, so switching dry run on or off starts the window over.
    if memory.dry_run.is_some_and(|previous| previous != config.dry_run) {
        memory.reset_automation();
    }
    memory.dry_run = Some(config.dry_run);

    let sleep_for = if !config.enabled {
        idle(&engine, memory).await
    } else if crate::storage::get_gemini_api_key().ok().flatten().is_none() {
        enter_blocked(state, services.notifier, memory, "Set a Gemini API key in Settings").await
    } else {
        memory.blocked_notified = None;
        let window = ctx.clock.window();
        if memory.window != Some(window) {
            match window {
                Window::Night => memory.night = NightMemory::default(),
                Window::Day => memory.day = DayMemory::default(),
            }
            memory.window = Some(window);
        }
        if state.force_check.swap(false, std::sync::atomic::Ordering::SeqCst) {
            memory.night.next_check_at = None;
            memory.day.next_check_at = None;
            memory.day.forced = true;
        }
        let sleep_for = match window {
            Window::Night => night_tick(&engine, memory, &reading, &ctx).await,
            Window::Day => day_tick(&engine, memory, &reading, &ctx).await,
        };
        match apply_smart_load(&engine, memory, &reading, &ctx).await {
            Some(cap) => sleep_for.min(cap),
            None => sleep_for,
        }
    };

    sync_plan_status(&engine, memory, &ctx).await;
    state.set_insights(build_insights(state, config, memory, &reading, &ctx)).await;
    sleep_for
}

async fn idle(engine: &Engine<'_>, memory: &mut EngineMemory) -> Duration {
    memory.reset_automation();
    engine
        .state
        .set_status(|status| {
            status.phase = Phase::Idle;
            status.reserve_soc = None;
            status.next_check_at = None;
            status.effective_mode = None;
        })
        .await;
    normal_interval(engine.config)
}

async fn sync_plan_status(engine: &Engine<'_>, memory: &EngineMemory, ctx: &Context) {
    let (effective, reserve, next_check) = match memory.window {
        Some(Window::Night) => (
            memory.night.effective_mode,
            matches!(memory.night.phase, NightPhase::Verifying | NightPhase::OnBattery)
                .then(|| memory.night.plan.as_ref().map(|plan| plan.reserve_soc))
                .flatten(),
            memory.night.next_check_at,
        ),
        Some(Window::Day) => (memory.day.effective_mode, None, memory.day.next_check_at),
        None => (None, None, None),
    };
    let nights = ctx.profile.nights_with_data;
    let night = (memory.window == Some(Window::Night)).then_some(&memory.night);
    let boost_until = night.and_then(|night| night.boost.as_ref().map(|boost| boost.until));
    let boosts = night.map_or(0, |night| night.boosts);
    engine
        .state
        .set_status(|status| {
            status.effective_mode = mode_label(effective);
            status.reserve_soc = reserve;
            status.next_check_at = next_check;
            status.history_nights = nights;
            status.boost_until = boost_until;
            status.boosts_tonight = boosts;
        })
        .await;
}

struct ClampedPlan {
    on_battery: bool,
    reserve_soc: f64,
    recheck_minutes: u32,
    confidence: f64,
    reason: String,
    smart_load_on_at: Option<String>,
}

fn clamp_night_plan(plan: agent::NightPlan, config: &AutomationConfig) -> ClampedPlan {
    let reserve = if plan.reserve_soc.is_finite() { plan.reserve_soc } else { MAX_RESERVE_SOC };
    ClampedPlan {
        on_battery: plan.mode.eq_ignore_ascii_case("sbg"),
        reserve_soc: reserve.clamp(config.min_soc_percent, MAX_RESERVE_SOC).round(),
        recheck_minutes: clamp_recheck(plan.recheck_minutes),
        confidence: plan.confidence,
        reason: plan.reason,
        smart_load_on_at: plan.smart_load_on_at,
    }
}

fn clamp_recheck(minutes: f64) -> u32 {
    let minutes = if minutes.is_finite() { minutes } else { MIN_RECHECK_MINUTES };
    minutes.clamp(MIN_RECHECK_MINUTES, MAX_RECHECK_MINUTES).round() as u32
}

fn is_summer(date: NaiveDate) -> bool {
    SUMMER_MONTHS.contains(&date.month())
}

/// Summer nights need the smart-load circuit (fans, AC) from the start of
/// the night; in winter the agent may start it later, but never after 23:00.
fn smart_load_on_time(clock: &Clock, night: &NightMemory) -> NaiveDateTime {
    let start = clock.night_start();
    if is_summer(clock.night()) {
        return start;
    }
    let latest = clock.night_on(SMART_LOAD_LATEST_HOUR).max(start);
    night.smart_load_on_at.unwrap_or(latest).clamp(start, latest)
}

fn remember_smart_load_time(night: &mut NightMemory, clock: &Clock, requested: Option<&str>) {
    if is_summer(clock.night()) {
        return;
    }
    if let Some(time) = requested.and_then(|text| NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()) {
        night.smart_load_on_at = Some(clock.night().and_time(time));
    }
}

fn smart_load_brief(clock: &Clock, reading: &EngineReading) -> SmartLoadBrief {
    let summer = is_summer(clock.night());
    SmartLoadBrief {
        season: if summer { "summer" } else { "winter" }.into(),
        currently_on: reading.smart_load,
        choose_on_time: !summer,
        earliest: hhmm(clock.night_start()),
        latest: hhmm(clock.night_on(SMART_LOAD_LATEST_HOUR).max(clock.night_start())),
    }
}

/// Sets the smart-load output once per window: off for the day, on for the
/// night. After that the user is free to change it by hand.
async fn apply_smart_load(engine: &Engine<'_>, memory: &mut EngineMemory, reading: &EngineReading, ctx: &Context) -> Option<Duration> {
    let now = ctx.clock.now;
    let (target, due, reason) = match memory.window? {
        Window::Day => {
            if memory.day.smart_load_done {
                return None;
            }
            (false, now, "smart load off for the day so the panels charge the battery".to_string())
        }
        Window::Night => {
            if memory.night.smart_load_done || memory.night.boost.is_some() {
                return None;
            }
            let due = smart_load_on_time(&ctx.clock, &memory.night);
            let reason = if is_summer(ctx.clock.night()) {
                "smart load on for the night (summer)".to_string()
            } else {
                format!("smart load on for the night (winter, from {})", hhmm(due))
            };
            (true, due, reason)
        }
    };
    if now < due {
        return Some(until(now, Some(due), normal_interval(engine.config)));
    }
    let done = |memory: &mut EngineMemory| match memory.window {
        Some(Window::Day) => memory.day.smart_load_done = true,
        Some(Window::Night) => memory.night.smart_load_done = true,
        None => {}
    };
    if reading.smart_load == Some(target) {
        done(memory);
        return None;
    }
    if !engine.config.dry_run {
        if let Err(error) = engine.client.write_smart_load(None, target).await {
            tracing::warn!("automation could not set smart load: {error}");
            return Some(FAST_TICK);
        }
    }
    let window = memory.window.unwrap_or(Window::Night);
    engine.record(now, window, if target { "smart_load_on" } else { "smart_load_off" }, None, None, None, true, &capitalize(&reason));
    engine.notify_action(&format!("turn {reason}")).await;
    done(memory);
    None
}

fn update_sim_soc(night: &mut NightMemory, reading: &EngineReading, dry_run: bool, now: NaiveDateTime) {
    if dry_run && night.effective_mode == Some(SBG_MODE) {
        if let (Some(soc), Some(last)) = (night.sim_soc, night.sim_updated_at) {
            let hours = (now - last).num_seconds().max(0) as f64 / 3600.0;
            let amps = estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v).unwrap_or(reading.discharge_a);
            let per_ah = if reading.rated_capacity_ah > 0.0 { 100.0 / reading.rated_capacity_ah } else { 0.0 };
            night.sim_soc = Some((soc - amps * hours * per_ah).max(0.0));
        }
        night.sim_updated_at = Some(now);
    }
}

/// While dry run pretends to be on battery, the real pack isn't draining, so
/// decisions use the simulated SOC instead.
fn night_soc(night: &NightMemory, reading: &EngineReading, dry_run: bool) -> Option<f64> {
    if dry_run && night.effective_mode == Some(SBG_MODE) {
        night.sim_soc.or(reading.soc)
    } else {
        reading.soc
    }
}

fn pack_volts(reading: &EngineReading) -> f64 {
    reading.batt_v.filter(|v| *v > 0.0).unwrap_or(NOMINAL_PACK_VOLTS)
}

fn current_load_w(reading: &EngineReading, discharge_a: f64) -> f64 {
    reading.load_w.unwrap_or(discharge_a * pack_volts(reading) * INVERTER_EFFICIENCY)
}

fn night_trajectory(
    ctx: &Context,
    reading: &EngineReading,
    config: &AutomationConfig,
    start: NaiveDateTime,
    start_soc: f64,
    current_load: Option<f64>,
) -> (Vec<SocPoint>, &'static str) {
    let end = ctx.clock.next_sunrise() + chrono::Duration::minutes((config.sunrise_buffer_hours * 60.0) as i64);
    let fallback = current_load.or(reading.load_w).unwrap_or(0.0);
    let has_history = ctx.profile.nights_with_data > 0;
    let start_hour = start.hour();
    let load_for = |hour: u32| {
        if hour == start_hour {
            if let Some(load) = current_load {
                return load;
            }
        }
        if has_history {
            ctx.profile.load_at(hour).unwrap_or(fallback)
        } else {
            fallback
        }
    };
    let points = soc_trajectory(start, end, start_soc, reading.rated_capacity_ah, pack_volts(reading), load_for);
    (points, if has_history { "history" } else { "current_load" })
}

fn sleeping_load_w(profile: &NightProfile) -> Option<f64> {
    [0, 1, 2, 3, 4, 5].iter().filter_map(|hour| profile.load_at(*hour)).reduce(f64::min)
}

fn weather_brief(summary: &WeatherSummary) -> WeatherBrief {
    WeatherBrief {
        next_day: summary.next_day.as_ref().map(|day| NextDayRow {
            date: day.date.format("%a %d %b").to_string(),
            radiation_kwh_m2: day.radiation_kwh_m2,
            sunshine_h: day.sunshine_h,
            cloud_pct: day.cloud_pct,
            rain_prob_pct: day.rain_prob_pct,
        }),
        recent_avg_radiation_kwh_m2: summary.recent_avg_radiation_kwh_m2,
        recent_days: recent_day_rows(summary),
        tonight_min_temp_c: summary.tonight_min_temp_c,
        recent_nights_min_temp_c: summary.recent_nights_min_temp_c,
        expected_pv_kwh_next_day: summary.expected_pv_kwh_next_day,
    }
}

fn recent_day_rows(summary: &WeatherSummary) -> Vec<RecentDayRow> {
    summary
        .recent_days
        .iter()
        .map(|day| RecentDayRow {
            date: day.date.format("%a %d %b").to_string(),
            radiation_kwh_m2: day.radiation_kwh_m2,
            max_soc: day.max_soc,
            full_at: day.full_at.clone(),
        })
        .collect()
}

fn build_night_input(
    ctx: &Context,
    reading: &EngineReading,
    night: &NightMemory,
    config: &AutomationConfig,
    soc: f64,
    discharge_a: f64,
    measured: bool,
) -> NightInput {
    let now = ctx.clock.now;
    let on_battery = night.effective_mode == Some(SBG_MODE);
    let load_now = current_load_w(reading, discharge_a);
    let (trajectory, basis) = night_trajectory(ctx, reading, config, now, soc, Some(load_now));
    let end = trajectory.last().map(|point| point.at).unwrap_or(now);
    let volts = pack_volts(reading);
    let remaining_hours: Vec<HourLoadRow> = ctx
        .profile
        .hourly
        .iter()
        .filter(|hour| {
            let at = now.date().and_hms_opt(hour.hour, 0, 0).unwrap_or(now);
            let at = if at <= now - chrono::Duration::hours(1) { at + chrono::Duration::days(1) } else { at };
            at <= end
        })
        .map(|hour| HourLoadRow {
            hour: format!("{:02}:00", hour.hour),
            load_w: hour.load_w.round(),
        })
        .collect();

    NightInput {
        now: now.format("%a %d %b %Y, %H:%M").to_string(),
        month: now.format("%B").to_string(),
        latitude: (ctx.latitude * 10.0).round() / 10.0,
        on_battery,
        on_battery_since: night.on_battery_since.map(hhmm),
        previous_plan: night.plan.as_ref().map(|plan| PreviousPlan {
            reserve_soc: plan.reserve_soc,
            reason: plan.reason.clone(),
        }),
        soc: soc.round(),
        rated_capacity_ah: reading.rated_capacity_ah,
        battery_v: reading.batt_v,
        floor_soc: config.min_soc_percent,
        load_w: reading.load_w.map(f64::round),
        pv_w: reading.pv_w.map(f64::round),
        discharge_a: (discharge_a * 10.0).round() / 10.0,
        discharge_measured: measured,
        sunrise: hhmm(ctx.clock.next_sunrise()),
        hours_until_sunrise: (((end - now).num_minutes() as f64) / 60.0 * 10.0).round() / 10.0,
        history_nights: ctx.profile.nights_with_data,
        typical_load_now_w: ctx.profile.load_at(now.hour()).map(f64::round),
        typical_hourly_load: remaining_hours,
        quiet_by: ctx.profile.quiet_by(config.night_start_hour).map(|hour| format!("{hour:02}:00")),
        trajectory: trajectory
            .iter()
            .map(|point| SocRow {
                time: hhmm(point.at),
                soc: point.soc.round(),
            })
            .collect(),
        trajectory_basis: basis.into(),
        soc_per_backup_hour: soc_per_hour_at(sleeping_load_w(&ctx.profile).unwrap_or(load_now), reading.rated_capacity_ah, volts)
            .map(|pct| (pct * 10.0).round() / 10.0),
        outages: ctx
            .outages
            .iter()
            .map(|outage| OutageRow {
                when: outage.start.format("%a %d %b, %H:%M").to_string(),
                minutes: outage.minutes.round(),
            })
            .collect(),
        weather: ctx.weather.as_ref().map(weather_brief),
        smart_load: smart_load_brief(&ctx.clock, reading),
    }
}

async fn night_tick(engine: &Engine<'_>, memory: &mut EngineMemory, reading: &EngineReading, ctx: &Context) -> Duration {
    let config = engine.config;
    let now = ctx.clock.now;
    if memory.night.effective_mode.is_none() {
        memory.night.effective_mode = reading.mode;
    }
    update_sim_soc(&mut memory.night, reading, config.dry_run, now);
    let soc = night_soc(&memory.night, reading, config.dry_run);

    let interrupted = guardrails::below_floor(soc, config.min_soc_percent)
        || guardrails::user_took_over(reading.mode, memory.night.last_written_mode, config.dry_run);
    if interrupted {
        if let Some(boost) = memory.night.boost.take() {
            if boost.smart_load_was_on {
                write_smart_load(engine, true).await;
            }
            set_boost_status(engine, &memory.night).await;
        }
    }

    if guardrails::below_floor(soc, config.min_soc_percent) {
        if memory.night.phase != NightPhase::Paused {
            if memory.night.effective_mode == Some(SBG_MODE) {
                revert_to_solar(engine, &mut memory.night, "switch to Solar — battery reached the floor").await;
            }
            memory.night.phase = NightPhase::Paused;
            engine.notify_info("Battery below floor — automation paused for the rest of the night").await;
        }
        engine.set_phase(Phase::Paused).await;
        return FAST_TICK;
    }

    if guardrails::user_took_over(reading.mode, memory.night.last_written_mode, config.dry_run) {
        if memory.night.phase != NightPhase::Paused {
            memory.night.phase = NightPhase::Paused;
            engine
                .notify_info("You changed the output mode — automation paused for the rest of the night")
                .await;
        }
        engine.set_phase(Phase::Paused).await;
        return normal_interval(config);
    }
    if !config.dry_run {
        memory.night.effective_mode = reading.mode;
    }

    if let Some(wait) = oven_boost(engine, memory, reading, ctx, soc).await {
        return wait;
    }

    let sleep_for = match memory.night.phase {
        NightPhase::Deciding => night_deciding(engine, memory, reading, ctx, soc).await,
        NightPhase::Verifying => night_verifying(engine, memory, reading, ctx, soc).await,
        NightPhase::OnBattery => night_on_battery(engine, memory, reading, ctx, soc).await,
        NightPhase::ReserveKept => {
            engine.set_phase(Phase::NightReserve).await;
            normal_interval(config)
        }
        NightPhase::Paused => {
            engine.set_phase(Phase::Paused).await;
            normal_interval(config)
        }
    };
    if boost_watch(engine.config, &memory.night) {
        sleep_for.min(BOOST_POLL)
    } else {
        sleep_for
    }
}

/// The boost only applies while the night plan has the house on grid.
fn boost_watch(config: &AutomationConfig, night: &NightMemory) -> bool {
    config.oven_boost_amps > 0.0
        && night.phase == NightPhase::Deciding
        && night.effective_mode == Some(SOLAR_MODE)
        && night.boosts < MAX_BOOSTS_PER_NIGHT
}

async fn set_boost_status(engine: &Engine<'_>, night: &NightMemory) {
    let until = night.boost.as_ref().map(|boost| boost.until);
    let boosts = night.boosts;
    engine
        .state
        .set_status(|status| {
            status.boost_until = until;
            status.boosts_tonight = boosts;
        })
        .await;
}

async fn write_smart_load(engine: &Engine<'_>, on: bool) -> bool {
    if engine.config.dry_run {
        return true;
    }
    match engine.client.write_smart_load(None, on).await {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!("oven boost could not set smart load: {error}");
            false
        }
    }
}

/// A big night-time load on grid (an oven) runs from the battery for a few
/// minutes with smart load off, then goes back to Solar. Returns the wait
/// while a boost is running so the regular night logic stays out of the way.
async fn oven_boost(
    engine: &Engine<'_>,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    ctx: &Context,
    soc: Option<f64>,
) -> Option<Duration> {
    let config = engine.config;
    let now = ctx.clock.now;
    let night = &mut memory.night;
    let reserve = night.last_reserve_soc.unwrap_or(config.min_soc_percent).max(config.min_soc_percent);

    if let Some(boost) = night.boost.clone() {
        let low = soc.is_some_and(|soc| soc <= reserve);
        if now < boost.until && !low {
            return Some(until(now, Some(boost.until), BOOST_POLL));
        }
        if !engine.write_mode(SOLAR_MODE).await {
            return Some(FAST_TICK);
        }
        if boost.smart_load_was_on {
            write_smart_load(engine, true).await;
        }
        if !config.dry_run {
            night.last_written_mode = Some(SOLAR_MODE);
        }
        night.effective_mode = Some(SOLAR_MODE);
        night.boost = None;
        night.last_boost_end = Some(now);
        let reason = if low { "Oven boost ended early — battery reached tonight's reserve." } else { "Oven boost finished — back on Solar." };
        engine.record(now, Window::Night, "solar", Some(reserve), None, None, true, reason);
        engine.state.notify_event(if config.dry_run { format!("[Dry run] {reason}") } else { reason.to_string() }).await;
        set_boost_status(engine, night).await;
        return Some(BOOST_POLL);
    }

    if !boost_watch(config, night) {
        return None;
    }
    let no_sun = reading.pv_w.is_none_or(|pv| pv < SUN_GONE_PV_W);
    let load_a = estimate_discharge_a(reading.load_w, reading.pv_w, Some(pack_volts(reading))).unwrap_or(0.0);
    let rested = night
        .last_boost_end
        .is_none_or(|end| now - end >= chrono::Duration::minutes(BOOST_GAP_MINUTES));
    let above_reserve = soc.is_some_and(|soc| soc > reserve + BOOST_RESERVE_MARGIN);
    if !(no_sun && rested && above_reserve && reading.grid_on() != Some(false) && load_a >= config.oven_boost_amps) {
        return None;
    }

    if !engine.write_mode(SBG_MODE).await {
        return None;
    }
    let smart_load_was_on = reading.smart_load == Some(true);
    if smart_load_was_on {
        write_smart_load(engine, false).await;
    }
    if !config.dry_run {
        night.last_written_mode = Some(SBG_MODE);
    }
    night.effective_mode = Some(SBG_MODE);
    let boost_until = now + chrono::Duration::minutes(BOOST_MINUTES);
    night.boost = Some(Boost { until: boost_until, smart_load_was_on });
    night.boosts += 1;
    let load_kw = reading.load_w.unwrap_or(0.0) / 1000.0;
    let reason = format!(
        "Big load ({load_kw:.1} kW, ~{load_a:.0} A) — running it from the battery for {BOOST_MINUTES} minutes with smart load off, then back to Solar."
    );
    engine.record(now, Window::Night, "boost_sbg", Some(reserve), Some(BOOST_MINUTES as u32), None, true, &reason);
    engine.notify_action(&format!("switch to battery (SBG) for {BOOST_MINUTES} min — oven-size load {load_kw:.1} kW")).await;
    set_boost_status(engine, night).await;
    Some(until(now, Some(boost_until), BOOST_POLL))
}

fn enter_verifying(night: &mut NightMemory, now: NaiveDateTime) {
    night.phase = NightPhase::Verifying;
    night.verify_started_at = Some(now);
    night.verify_samples.clear();
}

async fn revert_to_solar(engine: &Engine<'_>, night: &mut NightMemory, body: &str) -> bool {
    if !engine.write_mode(SOLAR_MODE).await {
        return false;
    }
    if !engine.config.dry_run {
        night.last_write_at = Some(Instant::now());
        night.last_written_mode = Some(SOLAR_MODE);
    }
    night.effective_mode = Some(SOLAR_MODE);
    night.sim_soc = None;
    night.sim_updated_at = None;
    night.on_battery_since = None;
    engine.notify_action(body).await;
    true
}

fn fmt_pct(value: f64) -> String {
    format!("{value:.0}%")
}

async fn night_deciding(
    engine: &Engine<'_>,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    ctx: &Context,
    soc: Option<f64>,
) -> Duration {
    let config = engine.config;
    let now = ctx.clock.now;
    engine.set_phase(Phase::NightDeciding).await;

    if memory.night.effective_mode == Some(SBG_MODE) {
        memory.night.on_battery_since = Some(now);
        if config.dry_run {
            memory.night.sim_soc = reading.soc;
            memory.night.sim_updated_at = Some(now);
        }
        enter_verifying(&mut memory.night, now);
        engine
            .set_reason("Already on battery at the start of the night — measuring the real draw before planning.")
            .await;
        engine.set_phase(Phase::NightVerifying).await;
        return FAST_TICK;
    }

    if memory.night.next_check_at.is_some_and(|next| now < next) {
        return until(now, memory.night.next_check_at, normal_interval(config));
    }
    let Some(soc) = soc else {
        engine.set_reason("Waiting for a battery SOC reading.").await;
        return normal_interval(config);
    };

    let discharge = estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v).unwrap_or(reading.discharge_a);
    let input = build_night_input(ctx, reading, &memory.night, config, soc, discharge, false);
    let plan = match engine.agent.run_night(input).await {
        Ok(plan) => clamp_night_plan(plan, config),
        Err(error) => {
            engine.ai_failure(&mut memory.night.ai_failure_notified, error, "Solar").await;
            return normal_interval(config);
        }
    };
    memory.night.ai_failure_notified = false;
    memory.night.last_reserve_soc = Some(plan.reserve_soc);
    remember_smart_load_time(&mut memory.night, &ctx.clock, plan.smart_load_on_at.as_deref());
    engine.set_reason(&plan.reason).await;
    let recheck_at = now + chrono::Duration::minutes(plan.recheck_minutes as i64);

    let stay_on_grid = |memory: &mut EngineMemory| {
        memory.night.next_check_at = Some(recheck_at);
    };

    if !plan.on_battery || soc <= plan.reserve_soc {
        engine.record(now, Window::Night, "solar", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), false, &plan.reason);
        stay_on_grid(memory);
        return until(now, memory.night.next_check_at, normal_interval(config));
    }
    if guardrails::engagement_cap_reached(memory.night.engagements) {
        engine.record(now, Window::Night, "sbg", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), false, &plan.reason);
        memory.night.phase = NightPhase::Paused;
        engine.notify_info("Reached tonight's battery engagement limit — staying on Solar").await;
        engine.set_phase(Phase::Paused).await;
        return normal_interval(config);
    }
    if !guardrails::can_write(reading.grid_on(), false, memory.night.last_write_at, Instant::now()) {
        engine.record(now, Window::Night, "sbg", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), false, &plan.reason);
        memory.night.next_check_at = Some(now + chrono::Duration::minutes(AI_RETRY_MINUTES));
        return until(now, memory.night.next_check_at, normal_interval(config));
    }
    if !engine.write_mode(SBG_MODE).await {
        return normal_interval(config);
    }

    engine.record(now, Window::Night, "sbg", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), true, &plan.reason);
    let night = &mut memory.night;
    if !config.dry_run {
        night.last_write_at = Some(Instant::now());
        night.last_written_mode = Some(SBG_MODE);
    } else {
        night.sim_soc = Some(soc);
        night.sim_updated_at = Some(now);
    }
    night.effective_mode = Some(SBG_MODE);
    night.engagements += 1;
    night.on_battery_since = Some(now);
    night.plan = Some(ActivePlan {
        reserve_soc: plan.reserve_soc,
        reason: plan.reason.clone(),
    });
    night.next_check_at = None;
    enter_verifying(night, now);
    engine
        .notify_action(&format!("switch to battery (SBG) until {} — {}", fmt_pct(plan.reserve_soc), plan.reason))
        .await;
    engine.set_phase(Phase::NightVerifying).await;
    FAST_TICK
}

async fn night_verifying(
    engine: &Engine<'_>,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    ctx: &Context,
    soc: Option<f64>,
) -> Duration {
    let config = engine.config;
    let now = ctx.clock.now;
    engine.set_phase(Phase::NightVerifying).await;

    let Some(started) = memory.night.verify_started_at else {
        memory.night.verify_started_at = Some(now);
        return FAST_TICK;
    };
    if now - started < VERIFY_SETTLE {
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
    let Some(soc) = soc else {
        return normal_interval(config);
    };
    let average = memory.night.verify_samples.iter().sum::<f64>() / memory.night.verify_samples.len() as f64;
    let input = build_night_input(ctx, reading, &memory.night, config, soc, average, !config.dry_run);
    match engine.agent.run_night(input).await {
        Ok(plan) => {
            remember_smart_load_time(&mut memory.night, &ctx.clock, plan.smart_load_on_at.as_deref());
            apply_on_battery_plan(engine, memory, clamp_night_plan(plan, config), soc, now, true).await
        }
        Err(error) => on_battery_ai_failure(engine, memory, error, now).await,
    }
}

async fn night_on_battery(
    engine: &Engine<'_>,
    memory: &mut EngineMemory,
    reading: &EngineReading,
    ctx: &Context,
    soc: Option<f64>,
) -> Duration {
    let config = engine.config;
    let now = ctx.clock.now;
    engine.set_phase(Phase::NightOnBattery).await;
    let reserve = memory.night.plan.as_ref().map(|plan| plan.reserve_soc).unwrap_or(config.min_soc_percent);
    let Some(soc) = soc else {
        return normal_interval(config);
    };

    if soc <= reserve {
        let reason = format!("Reached the {} reserve — keeping the rest as backup for tonight.", fmt_pct(reserve));
        if !revert_to_solar(engine, &mut memory.night, &format!("switch to Solar — {}", reason.to_lowercase())).await {
            return normal_interval(config);
        }
        engine.record(now, Window::Night, "solar", Some(reserve), None, None, true, &reason);
        engine.set_reason(&reason).await;
        memory.night.phase = NightPhase::ReserveKept;
        memory.night.next_check_at = None;
        engine.set_phase(Phase::NightReserve).await;
        return normal_interval(config);
    }

    if memory.night.next_check_at.is_none_or(|next| now >= next) {
        let discharge = if config.dry_run {
            estimate_discharge_a(reading.load_w, reading.pv_w, reading.batt_v).unwrap_or(reading.discharge_a)
        } else {
            reading.discharge_a
        };
        let input = build_night_input(ctx, reading, &memory.night, config, soc, discharge, !config.dry_run);
        return match engine.agent.run_night(input).await {
            Ok(plan) => {
                remember_smart_load_time(&mut memory.night, &ctx.clock, plan.smart_load_on_at.as_deref());
                apply_on_battery_plan(engine, memory, clamp_night_plan(plan, config), soc, now, false).await
            }
            Err(error) => on_battery_ai_failure(engine, memory, error, now).await,
        };
    }
    on_battery_sleep(config, now, memory.night.next_check_at, soc, reserve)
}

fn on_battery_sleep(config: &AutomationConfig, now: NaiveDateTime, next: Option<NaiveDateTime>, soc: f64, reserve: f64) -> Duration {
    let wait = until(now, next, normal_interval(config));
    if soc - reserve < NEAR_RESERVE_MARGIN {
        wait.min(NEAR_RESERVE_TICK)
    } else {
        wait
    }
}

async fn apply_on_battery_plan(
    engine: &Engine<'_>,
    memory: &mut EngineMemory,
    plan: ClampedPlan,
    soc: f64,
    now: NaiveDateTime,
    confirming: bool,
) -> Duration {
    let config = engine.config;
    memory.night.ai_failures = 0;
    memory.night.ai_failure_notified = false;
    memory.night.last_reserve_soc = Some(plan.reserve_soc);
    engine.set_reason(&plan.reason).await;
    let recheck_at = now + chrono::Duration::minutes(plan.recheck_minutes as i64);

    if !plan.on_battery || soc <= plan.reserve_soc {
        if !revert_to_solar(engine, &mut memory.night, &format!("switch to Solar — {}", plan.reason)).await {
            return normal_interval(config);
        }
        engine.record(now, Window::Night, "solar", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), true, &plan.reason);
        memory.night.phase = NightPhase::Deciding;
        memory.night.next_check_at = Some(recheck_at);
        memory.night.verify_samples.clear();
        engine.set_phase(Phase::NightDeciding).await;
        return until(now, memory.night.next_check_at, normal_interval(config));
    }

    engine.record(now, Window::Night, "sbg", Some(plan.reserve_soc), Some(plan.recheck_minutes), Some(plan.confidence), false, &plan.reason);
    memory.night.plan = Some(ActivePlan {
        reserve_soc: plan.reserve_soc,
        reason: plan.reason.clone(),
    });
    memory.night.phase = NightPhase::OnBattery;
    memory.night.next_check_at = Some(recheck_at);
    if confirming {
        engine
            .notify_info(&format!("On battery until {} — {}", fmt_pct(plan.reserve_soc), plan.reason))
            .await;
    }
    engine.set_phase(Phase::NightOnBattery).await;
    on_battery_sleep(config, now, memory.night.next_check_at, soc, plan.reserve_soc)
}

async fn on_battery_ai_failure(engine: &Engine<'_>, memory: &mut EngineMemory, error: AgentError, now: NaiveDateTime) -> Duration {
    let config = engine.config;
    tracing::warn!("automation agent call failed while on battery: {error}");
    memory.night.ai_failures += 1;
    if memory.night.ai_failures >= MAX_AI_FAILURES {
        if revert_to_solar(engine, &mut memory.night, "switch to Solar — AI unavailable, not running the battery unattended").await {
            memory.night.phase = NightPhase::Deciding;
            memory.night.ai_failures = 0;
            memory.night.next_check_at = Some(now + chrono::Duration::minutes(AI_RETRY_MINUTES));
            engine.set_phase(Phase::NightDeciding).await;
        }
        return normal_interval(config);
    }
    let reserve = memory.night.plan.as_ref().map(|plan| plan.reserve_soc).unwrap_or(config.min_soc_percent);
    if !memory.night.ai_failure_notified {
        memory.night.ai_failure_notified = true;
        engine
            .notify_info(&format!("AI unavailable — staying on battery until {} and retrying", fmt_pct(reserve)))
            .await;
    }
    if memory.night.phase == NightPhase::Verifying {
        enter_verifying(&mut memory.night, now);
        return FAST_TICK;
    }
    memory.night.next_check_at = Some(now + chrono::Duration::minutes(AI_RETRY_MINUTES));
    normal_interval(config)
}

fn day_projection(ctx: &Context, reading: &EngineReading, config: &AutomationConfig) -> Option<DayProjection> {
    let soc = reading.soc?;
    let radiation = |at: NaiveDateTime| ctx.forecast.as_ref().and_then(|forecast| forecast.radiation_during(at));
    let typical = |hour: u32| typical_day_load(ctx, hour);
    Some(project_day(DayProjectionInput {
        now: ctx.clock.now,
        sunset: ctx.clock.sunset_today,
        soc,
        rated_ah: reading.rated_capacity_ah,
        batt_v: reading.batt_v,
        charge_a: reading.charge_a.unwrap_or(0.0),
        max_charge_a: reading.max_charge_a,
        pv_now_w: reading.pv_w,
        pv_array_watts: config.pv_array_watts,
        current_load_w: reading.load_w,
        radiation_during: &radiation,
        typical_load_for_hour: &typical,
    }))
}

/// On SBG the battery covers whatever the sun doesn't. Measured when the
/// inverter really is on SBG; in dry run (inverter still on Solar) it's the
/// shortfall the battery *would* be covering.
fn draining_on_sbg(reading: &EngineReading) -> bool {
    if reading.mode == Some(SBG_MODE) {
        return reading.battery_a.is_some_and(|amps| amps < -DAY_DRAIN_A);
    }
    match (reading.load_w, reading.pv_w) {
        (Some(load), Some(pv)) => load - pv > DAY_DRAIN_A * pack_volts(reading),
        _ => false,
    }
}

fn sun_is_done(ctx: &Context, reading: &EngineReading) -> bool {
    let now = ctx.clock.now;
    let sunset = ctx.clock.sunset_today;
    now >= sunset
        || (sunset - now <= chrono::Duration::minutes(SUN_GONE_BEFORE_SUNSET_MINUTES)
            && reading.pv_w.is_some_and(|pv| pv < SUN_GONE_PV_W))
}

fn typical_day_load(ctx: &Context, hour: u32) -> Option<f64> {
    ctx.day_profile.iter().find(|h| h.hour == hour).map(|h| h.load_w)
}

fn build_day_input(
    ctx: &Context,
    reading: &EngineReading,
    projection: &DayProjection,
    effective: Option<u32>,
    draining: bool,
) -> DayInput {
    let next_hours = ctx
        .weather
        .as_ref()
        .map(|summary| {
            summary
                .hours_until_sunset
                .iter()
                .map(|hour| HourOutlookRow {
                    time: hhmm(hour.at - chrono::Duration::hours(1)),
                    cloud_pct: hour.cloud_pct,
                    radiation_w_m2: hour.radiation_w_m2,
                })
                .collect()
        })
        .unwrap_or_default();
    DayInput {
        now: ctx.clock.now.format("%a %d %b %Y, %H:%M").to_string(),
        soc: projection.soc_now.round(),
        rated_capacity_ah: reading.rated_capacity_ah,
        ah_to_full: projection.ah_to_full.round(),
        charge_a: (projection.charge_a * 10.0).round() / 10.0,
        max_charge_a: reading.max_charge_a,
        pv_w: reading.pv_w.map(f64::round),
        load_w: reading.load_w.map(f64::round),
        typical_load_now_w: typical_day_load(ctx, ctx.clock.now.hour()).map(f64::round),
        current_mode: mode_label(effective).unwrap_or_else(|| "Unknown".into()),
        battery_draining: draining,
        sunset: hhmm(ctx.clock.sunset_today),
        hours_of_sun_left: (projection.hours_of_sun_left * 10.0).round() / 10.0,
        projected_soc_at_sunset: projection.soc_at_sunset.round(),
        projected_full_at: projection.full_at.map(hhmm),
        projection_method: serde_json::to_value(projection.method)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        next_hours,
        recent_days: ctx.weather.as_ref().map(recent_day_rows).unwrap_or_default(),
        forecast_available: ctx.forecast.is_some(),
    }
}

async fn switch_day_mode(engine: &Engine<'_>, day: &mut DayMemory, reading: &EngineReading, target: u32, body: &str) -> bool {
    if !guardrails::can_write(reading.grid_on(), false, day.last_write_at, Instant::now()) {
        return false;
    }
    if !engine.write_mode(target).await {
        return false;
    }
    if !engine.config.dry_run {
        day.last_write_at = Some(Instant::now());
        day.last_written_mode = Some(target);
    }
    day.effective_mode = Some(target);
    engine.notify_action(body).await;
    true
}

async fn day_tick(engine: &Engine<'_>, memory: &mut EngineMemory, reading: &EngineReading, ctx: &Context) -> Duration {
    let config = engine.config;
    let now = ctx.clock.now;
    let day = &mut memory.day;
    if day.paused {
        engine.set_phase(Phase::Paused).await;
        return normal_interval(config);
    }
    if guardrails::user_took_over(reading.mode, day.last_written_mode, config.dry_run) {
        day.paused = true;
        engine.notify_info("You changed the output mode — automation paused until tonight").await;
        engine.set_phase(Phase::Paused).await;
        return normal_interval(config);
    }
    if day.effective_mode.is_none() || !config.dry_run {
        day.effective_mode = reading.mode;
    }
    engine.set_phase(Phase::Day).await;

    if sun_is_done(ctx, reading) {
        let night_start = hhmm(ctx.clock.night_start_on(now.date()));
        let sun = if now >= ctx.clock.sunset_today { "The sun has set" } else { "The sun is done for the day" };
        let reason = format!("{sun} — running on grid until night starts at {night_start}, keeping the battery for tonight.");
        if day.effective_mode == Some(SBG_MODE)
            && switch_day_mode(engine, day, reading, SOLAR_MODE, &format!("switch to Solar — {}", reason.to_lowercase())).await
        {
            engine.record(now, Window::Day, "solar", None, None, None, true, &reason);
        }
        engine.set_reason(&reason).await;
        return normal_interval(config);
    }

    if reading.battery_a.is_none() {
        engine
            .set_reason("Connect the battery (BMS) — the day check needs its charge current.")
            .await;
        return normal_interval(config);
    }
    let Some(projection) = day_projection(ctx, reading, config) else {
        return normal_interval(config);
    };
    let draining = day.effective_mode == Some(SBG_MODE) && draining_on_sbg(reading);
    day.drain_checks = if draining { day.drain_checks + 1 } else { 0 };
    if draining && day.drain_checks >= DRAIN_CHECKS_TO_SWITCH {
        let reason = "The battery is being drained with too little sun — switching to Solar so the grid carries the house and the battery is kept for tonight.";
        if switch_day_mode(engine, day, reading, SOLAR_MODE, &format!("switch to Solar — {}", reason.to_lowercase())).await {
            engine.record(now, Window::Day, "solar", None, Some(AFTER_DRAIN_RECHECK_MINUTES as u32), None, true, reason);
            day.next_check_at = Some(now + chrono::Duration::minutes(AFTER_DRAIN_RECHECK_MINUTES));
            day.drain_checks = 0;
        }
        engine.set_reason(reason).await;
        return until(now, day.next_check_at, normal_interval(config));
    }

    let implied = if projection.fills() && !draining { SBG_MODE } else { SOLAR_MODE };
    let forced = std::mem::take(&mut day.forced);
    if !forced && day.effective_mode == Some(implied) {
        return until(now, day.next_check_at, normal_interval(config));
    }
    if !forced && day.next_check_at.is_some_and(|next| now < next) {
        return until(now, day.next_check_at, normal_interval(config));
    }

    let input = build_day_input(ctx, reading, &projection, day.effective_mode, draining);
    let decision = match engine.agent.run_day(input).await {
        Ok(decision) => decision,
        Err(error) => {
            engine.ai_failure(&mut day.ai_failure_notified, error, "the current mode").await;
            return normal_interval(config);
        }
    };
    day.ai_failure_notified = false;
    let recheck = clamp_recheck(decision.recheck_minutes);
    day.next_check_at = Some(now + chrono::Duration::minutes(recheck as i64));
    engine.set_reason(&decision.reason).await;

    let target = if decision.mode.eq_ignore_ascii_case("sbg") { SBG_MODE } else { SOLAR_MODE };
    let applied = day.effective_mode != Some(target)
        && switch_day_mode(
            engine,
            day,
            reading,
            target,
            &format!("switch to {} — {}", output_mode_name(target), decision.reason),
        )
        .await;
    engine.record(now, Window::Day, &decision.mode, None, Some(recheck), Some(decision.confidence), applied, &decision.reason);
    until(now, day.next_check_at, normal_interval(config))
}

fn build_insights(
    state: &AutomationState,
    config: &AutomationConfig,
    memory: &EngineMemory,
    reading: &EngineReading,
    ctx: &Context,
) -> AutomationInsights {
    let clock = &ctx.clock;
    let window = clock.window();
    let night = &memory.night;
    let on_battery_plan = memory.window == Some(Window::Night)
        && matches!(night.phase, NightPhase::Verifying | NightPhase::OnBattery);
    let simulated_soc = (config.dry_run && on_battery_plan).then_some(night.sim_soc).flatten();
    let day = (window == Window::Day).then(|| day_projection(ctx, reading, config)).flatten();

    let (trajectory, preview, basis) = match (window, reading.soc) {
        (Window::Night, Some(soc)) => {
            let start_soc = simulated_soc.unwrap_or(soc);
            let (points, basis) = night_trajectory(ctx, reading, config, clock.now, start_soc, reading.load_w);
            (points, false, Some(basis))
        }
        (Window::Day, Some(soc)) => {
            let start_soc = day.as_ref().map(|d| d.soc_at_sunset).unwrap_or(soc).min(100.0);
            let (points, basis) = night_trajectory(ctx, reading, config, clock.night_start_on(clock.now.date()), start_soc, None);
            (points, true, Some(basis))
        }
        _ => (Vec::new(), false, None),
    };
    let reserve_soc = on_battery_plan.then(|| night.plan.as_ref().map(|plan| plan.reserve_soc)).flatten();
    let since = if window == Window::Night { clock.night_start() } else { clock.sunrise_today };
    let volts = pack_volts(reading);
    let backup_hours = reserve_soc.and_then(|reserve| {
        let per_hour = soc_per_hour_at(sleeping_load_w(&ctx.profile).or(reading.load_w)?, reading.rated_capacity_ah, volts)?;
        (per_hour > 0.0).then(|| ((reserve - config.min_soc_percent) / per_hour).max(0.0))
    });

    AutomationInsights {
        updated_at: Some(clock.now),
        window: Some(window.label()),
        sunrise: Some(clock.sunrise_today),
        sunset: Some(clock.sunset_today),
        night_start: Some(if window == Window::Night { clock.night_start() } else { clock.night_start_on(clock.now.date()) }),
        next_sunrise: Some(clock.next_sunrise()),
        soc: reading.soc,
        simulated_soc,
        floor_soc: config.min_soc_percent,
        reserve_soc,
        reserve_eta: reserve_soc.and_then(|reserve| first_at_or_below(&trajectory, reserve)),
        trajectory,
        trajectory_is_preview: preview,
        trajectory_basis: basis,
        actual_soc: history::soc_points(&ctx.samples, since)
            .into_iter()
            .map(|(at, soc)| SocPoint { at, soc })
            .collect(),
        backup_hours,
        routine: RoutineInsight {
            nights_with_data: ctx.profile.nights_with_data,
            typical: ctx.profile.hourly.clone(),
            tonight: history::tonight_hourly(&ctx.samples, clock.night()),
            quiet_by: ctx.profile.quiet_by(config.night_start_hour),
        },
        weather: ctx.weather.clone(),
        outages: ctx.outages.clone(),
        day,
        records: RecordsInfo {
            dir: state.history.root().display().to_string(),
            samples_this_month: state.history.rows_this_month("samples", clock.now.date()),
            decisions_this_month: state.history.rows_this_month("decisions", clock.now.date()),
        },
        grid: Some(reading.grid.clone()),
        smart_load: Some(SmartLoadInsight {
            season: if is_summer(clock.night()) { "summer" } else { "winter" },
            on: reading.smart_load,
            planned_on_at: (window == Window::Night).then(|| smart_load_on_time(clock, night)),
            night_on_at: if window == Window::Night { None } else { Some(clock.night_start_on(clock.now.date())) },
            done: match memory.window {
                Some(Window::Day) => memory.day.smart_load_done,
                Some(Window::Night) => night.smart_load_done,
                None => false,
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::history::HistoryStore;
    use crate::inverter::client::SolarCredentials;
    use crate::inverter::types::{GridBasis, GridStatus};
    use futures_util::future::BoxFuture;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn clock(now: NaiveDateTime) -> Clock {
        let date = now.date();
        Clock {
            now,
            sunrise_today: date.and_hms_opt(6, 0, 0).unwrap(),
            sunset_today: date.and_hms_opt(18, 0, 0).unwrap(),
            sunrise_tomorrow: date.succ_opt().unwrap().and_hms_opt(6, 0, 0).unwrap(),
            night_start_hour: 21,
        }
    }

    fn ctx(now: NaiveDateTime) -> Context {
        Context::build(clock(now), 31.5, Vec::new(), None, &AutomationConfig::default())
    }

    struct FakeAgent {
        night: StdMutex<VecDeque<Result<agent::NightPlan, String>>>,
        day: StdMutex<VecDeque<agent::DayDecision>>,
        night_calls: AtomicUsize,
        day_calls: AtomicUsize,
    }

    impl FakeAgent {
        fn new(night: Vec<Result<agent::NightPlan, String>>, day: Vec<agent::DayDecision>) -> Self {
            Self {
                night: StdMutex::new(night.into()),
                day: StdMutex::new(day.into()),
                night_calls: AtomicUsize::new(0),
                day_calls: AtomicUsize::new(0),
            }
        }
    }

    impl AgentRunner for FakeAgent {
        fn run_sun(&self, _input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>> {
            Box::pin(async { Err(AgentError::Timeout) })
        }

        fn run_night(&self, _input: NightInput) -> BoxFuture<'_, Result<agent::NightPlan, AgentError>> {
            self.night_calls.fetch_add(1, Ordering::SeqCst);
            let next = self.night.lock().unwrap().pop_front().expect("unexpected night call");
            Box::pin(async move { next.map_err(AgentError::ScriptFailed) })
        }

        fn run_day(&self, _input: DayInput) -> BoxFuture<'_, Result<agent::DayDecision, AgentError>> {
            self.day_calls.fetch_add(1, Ordering::SeqCst);
            let next = self.day.lock().unwrap().pop_front().expect("unexpected day call");
            Box::pin(async move { Ok(next) })
        }
    }

    #[derive(Default)]
    struct FakeNotifier {
        sent: StdMutex<Vec<String>>,
    }

    impl NotificationSink for FakeNotifier {
        fn send(&self, _title: &str, body: &str) {
            self.sent.lock().unwrap().push(body.to_string());
        }
    }

    struct Harness {
        state: AutomationState,
        client: SolarClient,
        notifier: FakeNotifier,
        config: AutomationConfig,
        dir: std::path::PathBuf,
    }

    impl Harness {
        fn new(dry_run: bool) -> Self {
            let config = AutomationConfig {
                enabled: true,
                dry_run,
                ..AutomationConfig::default()
            };
            let dir = std::env::temp_dir().join(format!("solar-hub-runner-{}", uuid::Uuid::new_v4()));
            Self {
                state: AutomationState::new(config.clone(), HistoryStore::new(dir.clone())),
                client: SolarClient::new(SolarCredentials::default()),
                notifier: FakeNotifier::default(),
                config,
                dir,
            }
        }

        fn engine<'a>(&'a self, agent: &'a dyn AgentRunner) -> Engine<'a> {
            Engine {
                state: &self.state,
                client: &self.client,
                notifier: &self.notifier,
                agent,
                config: &self.config,
            }
        }

        fn decisions(&self) -> Vec<DecisionRow> {
            self.state.history.recent_decisions(at(26, 0, 0).date(), 50)
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn reading(soc: f64, mode: u32, load_w: f64) -> EngineReading {
        EngineReading {
            soc: Some(soc),
            discharge_a: 0.0,
            charge_a: None,
            battery_a: Some(0.0),
            usable_capacity_ah: 50.0,
            rated_capacity_ah: 100.0,
            pv_w: Some(0.0),
            load_w: Some(load_w),
            batt_v: Some(51.2),
            grid: GridStatus {
                on: Some(true),
                basis: GridBasis::AcInput,
                voltage: Some(230.0),
                power_w: None,
            },
            mode: Some(mode),
            smart_load: Some(false),
            max_charge_a: Some(45.0),
            source: "bms",
        }
    }

    fn plan(mode: &str, reserve: f64, recheck: f64) -> Result<agent::NightPlan, String> {
        Ok(agent::NightPlan {
            mode: mode.into(),
            reserve_soc: reserve,
            recheck_minutes: recheck,
            confidence: 0.8,
            reason: format!("{mode} until {reserve}"),
            smart_load_on_at: Some("22:30".into()),
        })
    }

    fn night_memory() -> EngineMemory {
        EngineMemory {
            window: Some(Window::Night),
            ..EngineMemory::default()
        }
    }

    #[tokio::test]
    async fn after_dry_run_is_turned_off_the_night_is_planned_again_and_really_switches() {
        let dry = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("sbg", 41.0, 60.0), plan("sbg", 41.0, 60.0)], vec![]);
        let mut memory = night_memory();
        memory.dry_run = Some(true);
        night_tick(&dry.engine(&agent), &mut memory, &reading(80.0, SOLAR_MODE, 400.0), &ctx(at(26, 21, 0))).await;
        assert_eq!(memory.night.effective_mode, Some(SBG_MODE), "dry run only pretends");

        if memory.dry_run.is_some_and(|previous| previous) {
            memory.reset_automation();
            memory.window = Some(Window::Night);
        }
        let live = Harness::new(false);
        night_tick(&live.engine(&agent), &mut memory, &reading(80.0, SOLAR_MODE, 400.0), &ctx(at(26, 21, 20))).await;
        assert_eq!(agent.night_calls.load(Ordering::SeqCst), 2, "the live engine asks again instead of trusting the simulation");
    }

    #[test]
    fn window_splits_at_night_start_not_sunset() {
        assert_eq!(clock(at(26, 17, 0)).window(), Window::Day);
        assert_eq!(clock(at(26, 19, 0)).window(), Window::Day, "after sunset but before 21:00 is still day");
        assert_eq!(clock(at(26, 20, 59)).window(), Window::Day);
        assert_eq!(clock(at(26, 21, 0)).window(), Window::Night);
        assert_eq!(clock(at(26, 5, 0)).window(), Window::Night);
        assert_eq!(clock(at(26, 6, 0)).window(), Window::Day);
    }

    #[test]
    fn yesterdays_sun_times_just_after_midnight_still_mean_night() {
        let c = Clock::for_today(at(28, 0, 4), at(27, 5, 55), at(27, 17, 55), at(28, 5, 55), 21);
        assert_eq!(c.sunrise_today, at(28, 5, 55));
        assert_eq!(c.window(), Window::Night, "this is the bug that ran the house on grid all night");
        assert_eq!(c.next_sunrise(), at(28, 5, 55));
        let morning = Clock::for_today(at(28, 7, 0), at(27, 5, 55), at(27, 17, 55), at(28, 5, 55), 21);
        assert_eq!(morning.window(), Window::Day);
    }

    #[test]
    fn night_after_midnight_belongs_to_the_previous_evening() {
        let c = clock(at(27, 2, 0));
        assert_eq!(c.night(), at(26, 0, 0).date());
        assert_eq!(c.night_start(), at(26, 21, 0));
        assert_eq!(c.next_solar_day(), at(27, 0, 0).date());
    }

    #[tokio::test]
    async fn deciding_goes_on_battery_with_a_reserve_and_verifies() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("sbg", 35.0, 60.0)], vec![]);
        let mut memory = night_memory();
        night_tick(&h.engine(&agent), &mut memory, &reading(70.0, SOLAR_MODE, 600.0), &ctx(at(26, 21, 5))).await;

        assert_eq!(memory.night.phase, NightPhase::Verifying);
        assert_eq!(memory.night.effective_mode, Some(SBG_MODE));
        assert_eq!(memory.night.plan.as_ref().unwrap().reserve_soc, 35.0);
        assert_eq!(memory.night.sim_soc, Some(70.0));
        assert_eq!(h.state.status().await.phase, Phase::NightVerifying);
        let rows = h.decisions();
        assert_eq!((rows[0].mode.as_str(), rows[0].applied), ("sbg", true));
    }

    #[tokio::test]
    async fn solar_decision_waits_for_the_agents_recheck_time() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("solar", 40.0, 60.0)], vec![]);
        let mut memory = night_memory();
        let engine = h.engine(&agent);
        night_tick(&engine, &mut memory, &reading(45.0, SOLAR_MODE, 600.0), &ctx(at(26, 21, 5))).await;
        night_tick(&engine, &mut memory, &reading(45.0, SOLAR_MODE, 600.0), &ctx(at(26, 21, 40))).await;

        assert_eq!(agent.night_calls.load(Ordering::SeqCst), 1);
        assert_eq!(memory.night.phase, NightPhase::Deciding);
        assert_eq!(memory.night.next_check_at, Some(at(26, 22, 5)));
    }

    #[tokio::test]
    async fn reserve_is_clamped_to_the_floor() {
        let config = AutomationConfig::default();
        let clamped = clamp_night_plan(plan("sbg", 5.0, 500.0).unwrap(), &config);
        assert_eq!(clamped.reserve_soc, config.min_soc_percent);
        assert_eq!(clamped.recheck_minutes, 120);
    }

    #[tokio::test]
    async fn verifying_confirms_and_stays_on_battery() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("sbg", 30.0, 90.0)], vec![]);
        let mut memory = night_memory();
        memory.night.phase = NightPhase::Verifying;
        memory.night.effective_mode = Some(SBG_MODE);
        memory.night.verify_started_at = Some(at(26, 21, 0));
        memory.night.verify_samples = vec![10.0; 4];
        night_tick(&h.engine(&agent), &mut memory, &reading(70.0, SOLAR_MODE, 500.0), &ctx(at(26, 21, 10))).await;

        assert_eq!(memory.night.phase, NightPhase::OnBattery);
        assert_eq!(memory.night.next_check_at, Some(at(26, 22, 40)));
        assert_eq!(h.state.status().await.phase, Phase::NightOnBattery);
    }

    #[tokio::test]
    async fn verifying_can_change_its_mind_and_revert() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("solar", 50.0, 30.0)], vec![]);
        let mut memory = night_memory();
        memory.night.phase = NightPhase::Verifying;
        memory.night.effective_mode = Some(SBG_MODE);
        memory.night.verify_started_at = Some(at(26, 21, 0));
        memory.night.verify_samples = vec![30.0; 4];
        night_tick(&h.engine(&agent), &mut memory, &reading(70.0, SOLAR_MODE, 1500.0), &ctx(at(26, 21, 10))).await;

        assert_eq!(memory.night.phase, NightPhase::Deciding);
        assert_eq!(memory.night.effective_mode, Some(SOLAR_MODE));
    }

    #[tokio::test]
    async fn reaching_the_reserve_switches_to_solar_for_the_rest_of_the_night() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        memory.night.phase = NightPhase::OnBattery;
        memory.night.effective_mode = Some(SBG_MODE);
        memory.night.sim_soc = Some(39.0);
        memory.night.sim_updated_at = Some(at(27, 3, 0));
        memory.night.plan = Some(ActivePlan { reserve_soc: 40.0, reason: "test".into() });
        memory.night.next_check_at = Some(at(27, 4, 0));
        night_tick(&h.engine(&agent), &mut memory, &reading(80.0, SBG_MODE, 300.0), &ctx(at(27, 3, 0))).await;

        assert_eq!(memory.night.phase, NightPhase::ReserveKept);
        assert_eq!(memory.night.effective_mode, Some(SOLAR_MODE));
        assert_eq!(h.state.status().await.phase, Phase::NightReserve);
        assert!(h.notifier.sent.lock().unwrap()[0].contains("reserve"));
    }

    #[tokio::test]
    async fn dry_run_revert_is_not_undone_by_the_physical_inverter_still_on_sbg() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        memory.night.effective_mode = Some(SOLAR_MODE);
        memory.night.next_check_at = Some(at(26, 23, 0));
        night_tick(&h.engine(&agent), &mut memory, &reading(70.0, SBG_MODE, 600.0), &ctx(at(26, 22, 0))).await;

        assert_eq!(memory.night.phase, NightPhase::Deciding, "must not jump back into verifying");
        assert_eq!(agent.night_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn simulated_soc_drains_while_dry_run_is_on_battery() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        memory.night.phase = NightPhase::OnBattery;
        memory.night.effective_mode = Some(SBG_MODE);
        memory.night.sim_soc = Some(60.0);
        memory.night.sim_updated_at = Some(at(26, 22, 0));
        memory.night.plan = Some(ActivePlan { reserve_soc: 30.0, reason: "test".into() });
        memory.night.next_check_at = Some(at(27, 1, 0));
        night_tick(&h.engine(&agent), &mut memory, &reading(90.0, SOLAR_MODE, 512.0 * 0.9), &ctx(at(26, 23, 0))).await;

        let drained = 60.0 - memory.night.sim_soc.unwrap();
        assert!((drained - 10.0).abs() < 1e-6, "512 W·0.9 / 51.2 V / 0.9 = 10 A = 10%/h, drained {drained}");
        assert_eq!(memory.night.phase, NightPhase::OnBattery);
    }

    #[tokio::test]
    async fn repeated_ai_failures_on_battery_revert_to_solar() {
        let h = Harness::new(true);
        let failures = (0..3).map(|_| Err("down".to_string())).collect();
        let agent = FakeAgent::new(failures, vec![]);
        let mut memory = night_memory();
        memory.night.phase = NightPhase::OnBattery;
        memory.night.effective_mode = Some(SBG_MODE);
        memory.night.sim_soc = Some(70.0);
        memory.night.plan = Some(ActivePlan { reserve_soc: 30.0, reason: "test".into() });
        let engine = h.engine(&agent);
        for minute in [0, 20, 40] {
            memory.night.next_check_at = None;
            night_tick(&engine, &mut memory, &reading(70.0, SOLAR_MODE, 300.0), &ctx(at(26, 23, minute))).await;
        }
        assert_eq!(memory.night.phase, NightPhase::Deciding);
        assert_eq!(memory.night.effective_mode, Some(SOLAR_MODE));
    }

    #[tokio::test]
    async fn floor_breach_pauses_for_the_rest_of_the_night() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        night_tick(&h.engine(&agent), &mut memory, &reading(15.0, SOLAR_MODE, 300.0), &ctx(at(26, 22, 0))).await;
        assert_eq!(memory.night.phase, NightPhase::Paused);
    }

    #[tokio::test]
    async fn user_takeover_pauses_for_the_rest_of_the_night() {
        let h = Harness::new(false);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        memory.night.last_written_mode = Some(SBG_MODE);
        night_tick(&h.engine(&agent), &mut memory, &reading(80.0, SOLAR_MODE, 300.0), &ctx(at(26, 22, 0))).await;
        assert_eq!(memory.night.phase, NightPhase::Paused);
    }

    #[tokio::test]
    async fn engagement_cap_pauses_deciding() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("sbg", 30.0, 60.0)], vec![]);
        let mut memory = night_memory();
        memory.night.engagements = guardrails::MAX_ENGAGEMENTS_PER_NIGHT;
        night_tick(&h.engine(&agent), &mut memory, &reading(80.0, SOLAR_MODE, 300.0), &ctx(at(26, 22, 0))).await;
        assert_eq!(memory.night.phase, NightPhase::Paused);
    }

    fn day_decision(mode: &str) -> agent::DayDecision {
        agent::DayDecision {
            mode: mode.into(),
            recheck_minutes: 30.0,
            confidence: 0.7,
            reason: format!("go {mode}"),
        }
    }

    fn day_memory(effective: u32) -> EngineMemory {
        EngineMemory {
            window: Some(Window::Day),
            day: DayMemory {
                effective_mode: Some(effective),
                ..DayMemory::default()
            },
            ..EngineMemory::default()
        }
    }

    fn charging(soc: f64, mode: u32, amps: f64) -> EngineReading {
        EngineReading {
            charge_a: Some(amps),
            battery_a: Some(amps),
            ..reading(soc, mode, 400.0)
        }
    }

    #[tokio::test]
    async fn day_check_skips_the_llm_when_the_battery_will_fill_anyway() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = day_memory(SBG_MODE);
        day_tick(&h.engine(&agent), &mut memory, &charging(60.0, SBG_MODE, 10.0), &ctx(at(26, 11, 0))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 0);
        assert_eq!(memory.day.effective_mode, Some(SBG_MODE));
    }

    #[tokio::test]
    async fn day_check_asks_the_agent_when_the_battery_will_not_fill() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![day_decision("solar")]);
        let mut memory = day_memory(SBG_MODE);
        day_tick(&h.engine(&agent), &mut memory, &charging(40.0, SBG_MODE, 1.0), &ctx(at(26, 11, 0))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 1);
        assert_eq!(memory.day.effective_mode, Some(SOLAR_MODE));
        assert_eq!(memory.day.next_check_at, Some(at(26, 11, 30)));
    }

    #[tokio::test]
    async fn check_now_asks_the_agent_even_when_the_projection_agrees() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![day_decision("sbg")]);
        let mut memory = day_memory(SBG_MODE);
        memory.day.forced = true;
        memory.day.next_check_at = Some(at(26, 12, 0));
        day_tick(&h.engine(&agent), &mut memory, &charging(60.0, SBG_MODE, 10.0), &ctx(at(26, 11, 0))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 1);
        assert!(!memory.day.forced, "the force flag is used once");
    }

    fn dry_run_solar_reading(soc: f64, load_w: f64, pv_w: f64) -> EngineReading {
        EngineReading {
            battery_a: Some(-0.7),
            pv_w: Some(pv_w),
            ..reading(soc, SOLAR_MODE, load_w)
        }
    }

    #[tokio::test]
    async fn near_sunset_with_no_sun_goes_to_grid_without_asking() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = day_memory(SBG_MODE);
        day_tick(&h.engine(&agent), &mut memory, &dry_run_solar_reading(97.0, 101.0, 14.0), &ctx(at(26, 17, 43))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 0, "the agent can't keep SBG while the sun is done");
        assert_eq!(memory.day.effective_mode, Some(SOLAR_MODE));
    }

    #[tokio::test]
    async fn draining_on_sbg_for_two_checks_switches_to_solar_even_if_the_agent_said_sbg() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![day_decision("sbg")]);
        let mut memory = day_memory(SBG_MODE);
        let engine = h.engine(&agent);
        let cloudy = dry_run_solar_reading(80.0, 900.0, 300.0);
        day_tick(&engine, &mut memory, &cloudy, &ctx(at(26, 12, 0))).await;
        assert_eq!(memory.day.effective_mode, Some(SBG_MODE));
        day_tick(&engine, &mut memory, &cloudy, &ctx(at(26, 12, 15))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 1);
        assert_eq!(memory.day.effective_mode, Some(SOLAR_MODE));
        assert_eq!(memory.day.next_check_at, Some(at(26, 13, 15)));
    }

    #[tokio::test]
    async fn after_sunset_the_evening_goes_to_grid_without_asking() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = day_memory(SBG_MODE);
        day_tick(&h.engine(&agent), &mut memory, &charging(100.0, SBG_MODE, 0.0), &ctx(at(26, 18, 30))).await;
        assert_eq!(agent.day_calls.load(Ordering::SeqCst), 0);
        assert_eq!(memory.day.effective_mode, Some(SOLAR_MODE));
    }

    #[test]
    fn trajectory_uses_the_current_load_for_this_hour_then_the_profile() {
        let mut context = ctx(at(26, 21, 0));
        context.profile = NightProfile {
            nights_with_data: 3,
            hourly: (0..24)
                .map(|hour| history::HourLoad { hour, load_w: if hour == 21 || hour == 22 { 900.0 } else { 230.0 }, nights: 3 })
                .collect(),
        };
        let config = AutomationConfig::default();
        let (points, basis) = night_trajectory(&context, &reading(70.0, SOLAR_MODE, 1800.0), &config, at(26, 21, 0), 70.0, Some(1800.0));
        assert_eq!(basis, "history");
        let first_hour_drop = 70.0 - points[1].soc;
        let second_hour_drop = points[1].soc - points[2].soc;
        assert!(first_hour_drop > second_hour_drop * 1.9, "family still awake uses the measured load first");
    }

    fn in_month(month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[tokio::test]
    async fn summer_night_turns_smart_load_on_at_night_start() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        let cap = apply_smart_load(&h.engine(&agent), &mut memory, &reading(80.0, SOLAR_MODE, 300.0), &ctx(in_month(7, 10, 21, 0))).await;
        assert!(cap.is_none());
        assert!(memory.night.smart_load_done);
        assert!(h.notifier.sent.lock().unwrap()[0].contains("smart load on"));
    }

    #[tokio::test]
    async fn winter_night_waits_for_the_agents_time_then_turns_on() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![plan("solar", 40.0, 60.0)], vec![]);
        let mut memory = night_memory();
        let engine = h.engine(&agent);
        night_tick(&engine, &mut memory, &reading(45.0, SOLAR_MODE, 300.0), &ctx(at(26, 21, 0))).await;
        assert_eq!(memory.night.smart_load_on_at, Some(at(26, 22, 30)));

        let cap = apply_smart_load(&engine, &mut memory, &reading(45.0, SOLAR_MODE, 300.0), &ctx(at(26, 21, 0))).await;
        assert!(cap.is_some_and(|wait| wait <= Duration::from_secs(90 * 60)));
        assert!(!memory.night.smart_load_done);

        apply_smart_load(&engine, &mut memory, &reading(45.0, SOLAR_MODE, 300.0), &ctx(at(26, 22, 31))).await;
        assert!(memory.night.smart_load_done);
    }

    #[test]
    fn winter_smart_load_is_on_by_eleven_at_the_latest() {
        let c = clock(at(26, 21, 0));
        let mut night = NightMemory::default();
        assert_eq!(smart_load_on_time(&c, &night), at(26, 23, 0));
        night.smart_load_on_at = Some(at(27, 1, 0));
        assert_eq!(smart_load_on_time(&c, &night), at(26, 23, 0));
        night.smart_load_on_at = Some(at(26, 19, 0));
        assert_eq!(smart_load_on_time(&c, &night), at(26, 21, 0));
    }

    #[tokio::test]
    async fn day_turns_smart_load_off_once_and_leaves_manual_changes_alone() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = day_memory(SOLAR_MODE);
        let on = EngineReading { smart_load: Some(true), ..reading(80.0, SOLAR_MODE, 300.0) };
        let engine = h.engine(&agent);
        apply_smart_load(&engine, &mut memory, &on, &ctx(at(26, 7, 0))).await;
        apply_smart_load(&engine, &mut memory, &on, &ctx(at(26, 8, 0))).await;
        assert!(memory.day.smart_load_done);
        assert_eq!(h.notifier.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn smart_load_already_in_place_is_not_rewritten() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = night_memory();
        let on = EngineReading { smart_load: Some(true), ..reading(80.0, SOLAR_MODE, 300.0) };
        apply_smart_load(&h.engine(&agent), &mut memory, &on, &ctx(in_month(6, 10, 21, 30))).await;
        assert!(memory.night.smart_load_done);
        assert!(h.notifier.sent.lock().unwrap().is_empty());
    }

    fn oven(load_w: f64) -> EngineReading {
        EngineReading {
            smart_load: Some(true),
            ..reading(70.0, SOLAR_MODE, load_w)
        }
    }

    fn deciding_on_grid(reserve: f64) -> EngineMemory {
        let mut memory = night_memory();
        memory.night.effective_mode = Some(SOLAR_MODE);
        memory.night.last_reserve_soc = Some(reserve);
        memory.night.next_check_at = Some(at(26, 23, 30));
        memory
    }

    #[tokio::test]
    async fn oven_load_on_grid_runs_from_the_battery_for_three_minutes_then_back() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let mut memory = deciding_on_grid(40.0);
        let engine = h.engine(&agent);

        let wait = night_tick(&engine, &mut memory, &oven(3000.0), &ctx(at(26, 22, 0))).await;
        assert_eq!(memory.night.effective_mode, Some(SBG_MODE));
        assert_eq!(memory.night.boost.as_ref().unwrap().until, at(26, 22, 3));
        assert!(memory.night.boost.as_ref().unwrap().smart_load_was_on);
        assert!(wait <= Duration::from_secs(180));

        night_tick(&engine, &mut memory, &oven(3000.0), &ctx(at(26, 22, 1))).await;
        assert_eq!(memory.night.phase, NightPhase::Deciding, "the boost must not be mistaken for a night plan");
        assert!(memory.night.boost.is_some());

        night_tick(&engine, &mut memory, &oven(3000.0), &ctx(at(26, 22, 3))).await;
        assert_eq!(memory.night.effective_mode, Some(SOLAR_MODE), "back to Solar even if the load is still high");
        assert!(memory.night.boost.is_none());
        assert_eq!(agent.night_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_boost_for_normal_loads_or_at_the_reserve() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let engine = h.engine(&agent);
        let mut memory = deciding_on_grid(40.0);
        night_tick(&engine, &mut memory, &oven(800.0), &ctx(at(26, 22, 0))).await;
        assert!(memory.night.boost.is_none());

        let mut at_reserve = deciding_on_grid(69.0);
        night_tick(&engine, &mut at_reserve, &oven(3000.0), &ctx(at(26, 22, 0))).await;
        assert!(at_reserve.night.boost.is_none(), "never dips into tonight's backup");
    }

    #[tokio::test]
    async fn boosts_wait_between_runs_and_stop_after_four() {
        let h = Harness::new(true);
        let agent = FakeAgent::new(vec![], vec![]);
        let engine = h.engine(&agent);
        let mut memory = deciding_on_grid(40.0);
        memory.night.last_boost_end = Some(at(26, 21, 58));
        night_tick(&engine, &mut memory, &oven(3000.0), &ctx(at(26, 22, 0))).await;
        assert!(memory.night.boost.is_none(), "5-minute gap between boosts");

        memory.night.last_boost_end = None;
        memory.night.boosts = MAX_BOOSTS_PER_NIGHT;
        night_tick(&engine, &mut memory, &oven(3000.0), &ctx(at(26, 22, 0))).await;
        assert!(memory.night.boost.is_none());
    }
}
