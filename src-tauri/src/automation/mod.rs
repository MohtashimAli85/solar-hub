pub mod agent;
pub mod commands;
pub mod config;
pub mod guardrails;
pub mod history;
pub mod insights;
pub mod runner;
pub mod telemetry;
pub mod weather;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use chrono::NaiveDateTime;
use serde::Serialize;
use tauri::Manager;
use tokio::sync::{Mutex, Notify};

use config::AutomationConfig;
use history::HistoryStore;
use insights::AutomationInsights;

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
    pub reserve_soc: Option<f64>,
    pub next_check_at: Option<NaiveDateTime>,
    pub boost_until: Option<NaiveDateTime>,
    pub boosts_tonight: u32,
    pub history_nights: u32,
    pub history_dir: String,
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
            reserve_soc: None,
            next_check_at: None,
            boost_until: None,
            boosts_tonight: 0,
            history_nights: 0,
            history_dir: String::new(),
        }
    }
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
}

impl AutomationState {
    pub fn new(config: AutomationConfig, history: HistoryStore) -> Self {
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

    pub async fn set_status(&self, update: impl FnOnce(&mut AutomationStatus)) {
        let mut status = self.status.lock().await;
        update(&mut status);
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
