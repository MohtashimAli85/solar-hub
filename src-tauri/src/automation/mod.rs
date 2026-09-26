pub mod commands;
pub mod config;
pub mod engine;

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::Manager;
use tokio::sync::watch;
use tokio::sync::Mutex;

use config::AutomationConfig;

const MAX_LOGS: usize = 100;
pub const DISCHARGE_SAMPLE_WINDOW: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Day,
    Waiting,
    Probing,
    Holding,
    BackedOff,
    UserOverride,
    LowSoc,
}

/// The single piece of memory the engine carries between ticks.
pub struct EngineMemory {
    pub phase: Phase,
    pub previous_mode: Option<u32>,
    pub samples: Vec<f64>,
    pub last_write_at: Option<Instant>,
    pub probe_until: Option<Instant>,
    pub engaged_at: Option<Instant>,
    pub consecutive_shortfalls: u32,
}

impl Default for EngineMemory {
    fn default() -> Self {
        Self {
            phase: Phase::Day,
            previous_mode: None,
            samples: Vec::new(),
            last_write_at: None,
            probe_until: None,
            engaged_at: None,
            consecutive_shortfalls: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LiveStatus {
    pub soc: Option<f64>,
    pub battery_current_a: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub grid_on: Option<bool>,
    pub smart_load: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Estimate {
    pub discharge_a: Option<f64>,
    pub runtime_h: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationLogEntry {
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationWarning {
    pub timestamp: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationStatus {
    pub enabled: bool,
    pub last_check: Option<String>,
    pub current_mode_name: Option<String>,
    pub warning: Option<AutomationWarning>,
    pub automation_engaged: bool,
    pub phase: Phase,
    pub next_check: Option<String>,
    pub live: LiveStatus,
    pub estimate: Estimate,
    pub verified: Estimate,
    pub required_h: Option<f64>,
    pub data_source: Option<String>,
    pub logs: Vec<AutomationLogEntry>,
}

impl Default for AutomationStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            last_check: None,
            current_mode_name: None,
            warning: None,
            automation_engaged: false,
            phase: Phase::Day,
            next_check: None,
            live: LiveStatus::default(),
            estimate: Estimate::default(),
            verified: Estimate::default(),
            required_h: None,
            data_source: None,
            logs: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct AutomationState {
    pub config: Arc<Mutex<AutomationConfig>>,
    pub status: Arc<Mutex<AutomationStatus>>,
    pub memory: Arc<Mutex<EngineMemory>>,
    pub check_tx: watch::Sender<bool>,
}

impl AutomationState {
    pub fn new(config: AutomationConfig) -> Self {
        let (check_tx, _) = watch::channel(false);
        Self {
            config: Arc::new(Mutex::new(config)),
            status: Arc::new(Mutex::new(AutomationStatus::default())),
            memory: Arc::new(Mutex::new(EngineMemory::default())),
            check_tx,
        }
    }

    pub async fn config(&self) -> AutomationConfig {
        self.config.lock().await.clone()
    }

    pub async fn status(&self) -> AutomationStatus {
        self.status.lock().await.clone()
    }

    pub async fn log(&self, message: impl Into<String>) {
        let message = message.into();
        let mut status = self.status.lock().await;
        status.logs.insert(
            0,
            AutomationLogEntry {
                timestamp: chrono::Local::now().to_rfc3339(),
                message,
            },
        );
        status.logs.truncate(MAX_LOGS);
    }

    /// Skips the message when it is identical to the newest log entry. Safety
    /// net for anything that would otherwise repeat — the engine's real fix is
    /// to only log transitions.
    pub async fn log_change(&self, message: impl Into<String>) {
        let message = message.into();
        let mut status = self.status.lock().await;
        let newest = status.logs.first().map(|entry| entry.message.as_str());
        if newest == Some(message.as_str()) {
            return;
        }
        status.logs.insert(
            0,
            AutomationLogEntry {
                timestamp: chrono::Local::now().to_rfc3339(),
                message,
            },
        );
        status.logs.truncate(MAX_LOGS);
    }

    pub async fn set_status(&self, update: impl FnOnce(&mut AutomationStatus)) {
        let mut status = self.status.lock().await;
        update(&mut status);
    }

    /// Persists the config to disk and updates the shared state in one step.
    pub async fn apply_config(
        &self,
        app: &tauri::AppHandle,
        mut config: AutomationConfig,
    ) -> Result<AutomationConfig, String> {
        config.clamp_for_ui();
        *self.config.lock().await = config.clone();
        crate::storage::save_automation_config(app, &config)?;
        self.set_status(|status| status.enabled = config.enabled).await;
        if let Some(notifier) = app.try_state::<crate::notifications::Notifier>() {
            notifier.set_enabled(config.notifications_enabled);
        }
        Ok(config.clone())
    }
}

pub fn push_discharge_sample(samples: &mut Vec<f64>, sample: f64) {
    samples.push(sample);
    if samples.len() > DISCHARGE_SAMPLE_WINDOW {
        let excess = samples.len() - DISCHARGE_SAMPLE_WINDOW;
        samples.drain(..excess);
    }
}

pub fn discharge_average(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        0.0
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_average_keeps_last_samples() {
        let mut samples = Vec::new();
        for value in [10.0, 20.0, 30.0, 40.0, 50.0, 60.0] {
            push_discharge_sample(&mut samples, value);
        }
        assert_eq!(samples, vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
        assert!((discharge_average(&samples) - 35.0).abs() < 1e-9);

        push_discharge_sample(&mut samples, 70.0);
        assert_eq!(samples, vec![20.0, 30.0, 40.0, 50.0, 60.0, 70.0]);
        assert!((discharge_average(&samples) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_damps_a_load_spike() {
        let mut samples = Vec::new();
        for _ in 0..2 {
            push_discharge_sample(&mut samples, 15.0);
        }
        push_discharge_sample(&mut samples, 45.0);
        assert!((discharge_average(&samples) - 25.0).abs() < 1e-9);
    }

    #[test]
    fn empty_averages_to_zero() {
        assert_eq!(discharge_average(&[]), 0.0);
    }
}