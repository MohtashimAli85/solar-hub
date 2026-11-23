pub mod agent;
pub mod commands;
pub mod config;
pub mod guardrails;
pub mod history;
pub mod insights;
pub mod loads;
pub mod runner;
pub mod solar;
pub mod telemetry;
pub mod water;
pub mod weather;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use tauri::Manager;
use tokio::sync::{Mutex, Notify};

use config::AutomationConfig;
use history::HistoryStore;
use insights::AutomationInsights;
use loads::LoadsStatus;
use water::WaterStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Day,
    NightDeciding,
    NightVerifying,
    NightOnBattery,
    NightReserve,
    Paused,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastEvent {
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationStatus {
    pub enabled: bool,
    pub dry_run: bool,
    pub phase: Phase,
    pub mode_name: Option<String>,
    pub effective_mode: Option<String>,
    pub last_event: Option<LastEvent>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub blocked_reason: Option<String>,
    pub ai_reason: Option<String>,
    /// The model behind `ai_reason`; none when the engine's own rule decided.
    pub ai_model: Option<String>,
    pub reserve_soc: Option<f64>,
    pub next_check_at: Option<NaiveDateTime>,
    pub boost_until: Option<NaiveDateTime>,
    pub boosts_tonight: u32,
    pub history_nights: u32,
    pub history_dir: String,
    pub water: WaterStatus,
    pub loads: LoadsStatus,
}

impl Default for AutomationStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            phase: Phase::Idle,
            mode_name: None,
            effective_mode: None,
            last_event: None,
            sunrise: None,
            sunset: None,
            blocked_reason: None,
            ai_reason: None,
            ai_model: None,
            reserve_soc: None,
            next_check_at: None,
            boost_until: None,
            boosts_tonight: 0,
            history_nights: 0,
            history_dir: String::new(),
            water: WaterStatus::default(),
            loads: LoadsStatus::default(),
        }
    }
}

/// The days the app remembers across restarts ("the tank was filled today").
#[derive(Debug, Clone, Default)]
pub struct SavedDays {
    pub water_done_on: Option<NaiveDate>,
    pub geyser_done_on: Option<NaiveDate>,
    pub ev_skip_on: Option<NaiveDate>,
    pub hotplate_on: Option<NaiveDate>,
    pub hotplate_skip_on: Option<NaiveDate>,
    pub ev_checked_on: Option<NaiveDate>,
}

#[derive(Clone)]
pub struct AutomationState {
    pub config: Arc<Mutex<AutomationConfig>>,
    pub status: Arc<Mutex<AutomationStatus>>,
    pub insights: Arc<Mutex<AutomationInsights>>,
    pub history: HistoryStore,
    pub wake: Arc<Notify>,
    /// Set by "Check now": the next tick asks the agent even if nothing is due.
    pub force_check: Arc<AtomicBool>,
    pub refresh_forecast: Arc<AtomicBool>,
    pub water_done_on: Arc<Mutex<Option<NaiveDate>>>,
    pub geyser_done_on: Arc<Mutex<Option<NaiveDate>>>,
    pub ev_skip_on: Arc<Mutex<Option<NaiveDate>>>,
    /// The last day the family cooked on the hotplate.
    pub hotplate_on: Arc<Mutex<Option<NaiveDate>>>,
    pub hotplate_skip_on: Arc<Mutex<Option<NaiveDate>>>,
    /// The last day the scooter's daily live read was done.
    pub ev_checked_on: Arc<Mutex<Option<NaiveDate>>>,
    /// The family's answer to a question on the solar loads card, picked up
    /// by the next tick.
    pub loads_answer: Arc<Mutex<Option<loads::LoadsAnswer>>>,
    pub loads_edits: Arc<Mutex<Vec<loads::LoadEdit>>>,
    pub geyser_request: Arc<Mutex<Option<loads::GeyserRequest>>>,
    /// Bumped whenever the family changes a load, so an AI round planned on
    /// the old state is dropped instead of making them wait for it.
    pub loads_changed: Arc<Notify>,
    pub loads_generation: Arc<std::sync::atomic::AtomicU64>,
    /// A mode write that errored, and when: the inverter may still have taken
    /// it, so seeing that mode next isn't the family taking over.
    pub failed_mode_write: Arc<std::sync::Mutex<Option<(u32, std::time::Instant)>>>,
}

impl AutomationState {
    pub fn new(config: AutomationConfig, history: HistoryStore, days: SavedDays) -> Self {
        let status = AutomationStatus {
            enabled: config.enabled,
            dry_run: config.dry_run,
            history_dir: history.root().display().to_string(),
            ..AutomationStatus::default()
        };
        Self {
            config: Arc::new(Mutex::new(config)),
            status: Arc::new(Mutex::new(status)),
            insights: Arc::new(Mutex::new(AutomationInsights::default())),
            history,
            wake: Arc::new(Notify::new()),
            force_check: Arc::new(AtomicBool::new(false)),
            refresh_forecast: Arc::new(AtomicBool::new(false)),
            water_done_on: Arc::new(Mutex::new(days.water_done_on)),
            geyser_done_on: Arc::new(Mutex::new(days.geyser_done_on)),
            ev_skip_on: Arc::new(Mutex::new(days.ev_skip_on)),
            hotplate_on: Arc::new(Mutex::new(days.hotplate_on)),
            hotplate_skip_on: Arc::new(Mutex::new(days.hotplate_skip_on)),
            ev_checked_on: Arc::new(Mutex::new(days.ev_checked_on)),
            loads_answer: Arc::new(Mutex::new(None)),
            loads_edits: Arc::new(Mutex::new(Vec::new())),
            geyser_request: Arc::new(Mutex::new(None)),
            loads_changed: Arc::new(Notify::new()),
            loads_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            failed_mode_write: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub async fn config(&self) -> AutomationConfig {
        self.config.lock().await.clone()
    }

    pub async fn status(&self) -> AutomationStatus {
        self.status.lock().await.clone()
    }

    pub async fn insights(&self) -> AutomationInsights {
        self.insights.lock().await.clone()
    }

    pub async fn set_insights(&self, insights: AutomationInsights) {
        *self.insights.lock().await = insights;
    }

    pub fn loads_touched(&self) {
        self.loads_generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.loads_changed.notify_waiters();
        self.wake.notify_one();
    }

    pub async fn set_status(&self, update: impl FnOnce(&mut AutomationStatus)) {
        let mut status = self.status.lock().await;
        update(&mut status);
    }

    pub async fn water_done_on(&self) -> Option<NaiveDate> {
        *self.water_done_on.lock().await
    }

    pub async fn geyser_done_on(&self) -> Option<NaiveDate> {
        *self.geyser_done_on.lock().await
    }

    pub async fn ev_skip_on(&self) -> Option<NaiveDate> {
        *self.ev_skip_on.lock().await
    }

    pub async fn hotplate_on(&self) -> Option<NaiveDate> {
        *self.hotplate_on.lock().await
    }

    pub async fn hotplate_skip_on(&self) -> Option<NaiveDate> {
        *self.hotplate_skip_on.lock().await
    }

    pub async fn ev_checked_on(&self) -> Option<NaiveDate> {
        *self.ev_checked_on.lock().await
    }

    pub async fn notify_event(&self, message: impl Into<String>) {
        let message = message.into();
        self.set_status(|status| {
            status.last_event = Some(LastEvent {
                timestamp: chrono::Local::now().to_rfc3339(),
                message,
            });
        })
        .await;
    }

    pub async fn apply_config(
        &self,
        app: &tauri::AppHandle,
        mut config: AutomationConfig,
    ) -> Result<AutomationConfig, String> {
        config.clamp_for_ui();
        *self.config.lock().await = config.clone();
        crate::storage::save_automation_config(app, &config)?;
        self.set_status(|status| {
            status.enabled = config.enabled;
            status.dry_run = config.dry_run;
        })
        .await;
        if let Some(notifier) = app.try_state::<crate::notifications::Notifier>() {
            notifier.set_enabled(config.notifications_enabled);
        }
        self.wake.notify_one();
        Ok(config)
    }
}
