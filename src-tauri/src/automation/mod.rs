pub mod agent;
pub mod commands;
pub mod config;
pub mod guardrails;
pub mod runner;
pub mod telemetry;

use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;
use tokio::sync::{Mutex, Notify};

use config::AutomationConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    NightDeciding,
    NightVerifying,
    NightHolding,
    Paused,
    Morning,
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
    pub last_event: Option<LastEvent>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub blocked_reason: Option<String>,
}

impl Default for AutomationStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            phase: Phase::Idle,
            mode_name: None,
            last_event: None,
            sunrise: None,
            sunset: None,
            blocked_reason: None,
        }
    }
}

#[derive(Clone)]
pub struct AutomationState {
    pub config: Arc<Mutex<AutomationConfig>>,
    pub status: Arc<Mutex<AutomationStatus>>,
    pub wake: Arc<Notify>,
}

impl AutomationState {
    pub fn new(config: AutomationConfig) -> Self {
        let status = AutomationStatus {
            enabled: config.enabled,
            dry_run: config.dry_run,
            ..AutomationStatus::default()
        };
        Self {
            config: Arc::new(Mutex::new(config)),
            status: Arc::new(Mutex::new(status)),
            wake: Arc::new(Notify::new()),
        }
    }

    pub async fn config(&self) -> AutomationConfig {
        self.config.lock().await.clone()
    }

    pub async fn status(&self) -> AutomationStatus {
        self.status.lock().await.clone()
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
