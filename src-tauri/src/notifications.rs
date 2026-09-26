use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::battery::types::BatterySnapshot;

const LOW_SOC_FLOOR_PERCENT: f64 = 20.0;
const LOW_SOC_CLEAR_PERCENT: f64 = 25.0;

/// A partial telemetry reading. `None` fields mean "no data this round" and are
/// merged into whatever is already latched.
#[derive(Debug, Clone, Default)]
pub struct Reading {
    /// State of charge, 0–100.
    pub soc: Option<f64>,
    /// Battery current in amps, positive = charging, negative = discharging.
    pub current_a: Option<f64>,
    /// Whether the grid input is present.
    pub grid_on: Option<bool>,
    /// Hottest cell temperature in °C.
    pub max_temp_c: Option<f64>,
    /// Raw JBD protection status; `Some(0)` means no protection is active.
    pub protection: Option<u16>,
}

impl Reading {
    pub fn merge(&mut self, patch: &Reading) {
        if patch.soc.is_some() {
            self.soc = patch.soc;
        }
        if patch.current_a.is_some() {
            self.current_a = patch.current_a;
        }
        if patch.grid_on.is_some() {
            self.grid_on = patch.grid_on;
        }
        if patch.max_temp_c.is_some() {
            self.max_temp_c = patch.max_temp_c;
        }
        if patch.protection.is_some() {
            self.protection = patch.protection;
        }
    }

    pub fn from_bms(snapshot: &BatterySnapshot) -> Reading {
        let max_temp = snapshot
            .temperatures
            .iter()
            .copied()
            .fold(None, |acc, temp| Some(acc.map_or(temp, |current: f64| current.max(temp))));
        Reading {
            soc: Some(snapshot.soc as f64),
            current_a: Some(snapshot.current),
            grid_on: None,
            max_temp_c: max_temp,
            protection: Some(snapshot.protection_status),
        }
    }
}

struct Rule {
    key: &'static str,
    title: &'static str,
    fires: fn(&Reading) -> Option<bool>,
    clears: fn(&Reading) -> Option<bool>,
    body: fn(&Reading) -> String,
}

const RULES: &[Rule] = &[
    Rule {
        key: "battery_full",
        title: "Battery full",
        fires: |reading| reading.soc.map(|soc| soc >= 100.0),
        clears: |reading| reading.soc.map(|soc| soc <= 95.0),
        body: |reading| format!("Battery is fully charged ({:.0}%).", reading.soc.unwrap_or(0.0)),
    },
    Rule {
        key: "high_discharge",
        title: "High discharge",
        fires: |reading| reading.current_a.map(|current| current <= -40.0),
        clears: |reading| reading.current_a.map(|current| current >= -30.0),
        body: |reading| {
            format!(
                "Battery pulling {:.1} A.",
                reading.current_a.unwrap_or(0.0).abs()
            )
        },
    },
    Rule {
        key: "half_battery",
        title: "Half battery",
        fires: |reading| match (reading.soc, reading.current_a) {
            (Some(soc), Some(current)) => Some(soc <= 50.0 && current < 0.0),
            _ => None,
        },
        clears: |reading| reading.soc.map(|soc| soc >= 55.0),
        body: |reading| {
            format!(
                "Battery at {:.0}% and discharging.",
                reading.soc.unwrap_or(0.0)
            )
        },
    },
    Rule {
        key: "low_soc",
        title: "Low battery",
        fires: |reading| match (reading.soc, reading.current_a) {
            (Some(soc), Some(current)) => Some(soc <= LOW_SOC_FLOOR_PERCENT && current < 0.0),
            _ => None,
        },
        clears: |reading| reading.soc.map(|soc| soc >= LOW_SOC_CLEAR_PERCENT),
        body: |reading| {
            format!(
                "Battery low at {:.0}% while discharging.",
                reading.soc.unwrap_or(0.0)
            )
        },
    },
    Rule {
        key: "grid_off",
        title: "Grid off",
        fires: |reading| reading.grid_on.map(|on| !on),
        clears: |reading| reading.grid_on,
        body: |_| "Grid input has dropped; running on battery.".to_string(),
    },
    Rule {
        key: "grid_back",
        title: "Grid back",
        fires: |reading| reading.grid_on,
        clears: |reading| reading.grid_on.map(|on| !on),
        body: |_| "Grid input has returned.".to_string(),
    },
    Rule {
        key: "battery_hot",
        title: "Battery temperature",
        fires: |reading| reading.max_temp_c.map(|temp| temp >= 45.0),
        clears: |reading| reading.max_temp_c.map(|temp| temp <= 40.0),
        body: |reading| {
            format!(
                "Battery cell temperature is {:.0}°C.",
                reading.max_temp_c.unwrap_or(0.0)
            )
        },
    },
    Rule {
        key: "bms_protection",
        title: "BMS protection",
        fires: |reading| reading.protection.map(|protection| protection != 0),
        clears: |reading| reading.protection.map(|protection| protection == 0),
        body: |_| "BMS protection is active.".to_string(),
    },
];

/// Advance one rule's latch given this round's `fires` and (when latched)
/// `clears`. Returns whether an alert should be emitted right now.
///
/// The first time a rule sees data it is primed to its current value without
/// firing; an unlatched rule fires on the false → true edge; a latched rule
/// only unlatches when its `clears` condition becomes true (hysteresis).
fn step_rule(latch: &mut Option<bool>, fires: Option<bool>, clears: Option<bool>) -> bool {
    let Some(fired) = fires else {
        return false;
    };
    let latched = latch.get_or_insert(fired);
    if !*latched && fired {
        *latched = true;
        return true;
    }
    if *latched && clears == Some(true) {
        *latched = false;
    }
    false
}

struct NotifierInner {
    app: AppHandle,
    enabled: AtomicBool,
    latch: StdMutex<(Reading, HashMap<&'static str, Option<bool>>)>,
}

#[derive(Clone)]
pub struct Notifier {
    inner: Arc<NotifierInner>,
}

impl Notifier {
    pub fn new(app: AppHandle) -> Self {
        Self {
            inner: Arc::new(NotifierInner {
                app,
                enabled: AtomicBool::new(true),
                latch: StdMutex::new((Reading::default(), HashMap::new())),
            }),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Merges a patch and evaluates every rule. Battery alerts arrive from the
    /// BMS publisher in real time; the engine feeds grid status on each tick.
    pub async fn observe(&self, patch: Reading) {
        let mut guard = self
            .inner
            .latch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.0.merge(&patch);
        if !self.inner.enabled.load(Ordering::Relaxed) {
            return;
        }
        let latest = guard.0.clone();
        for rule in RULES {
            let fires = (rule.fires)(&latest);
            let clears = (rule.clears)(&latest);
            let should_fire = step_rule(guard.1.entry(rule.key).or_insert(None), fires, clears);
            if should_fire {
                let body = (rule.body)(&latest);
                self.send(rule.title, &body);
            }
        }
    }

    /// Imperative alert used by the automation engine for mode switches.
/// Every notification is also written to the automation log. The desktop
/// banner is skipped while notifications are disabled in the config.
    pub fn send(&self, title: &str, body: &str) {
        self.emit(title, body, false);
    }

    /// Manual test alert from the UI. Always shows the banner so delivery can
    /// be verified even while notifications are disabled in the config.
    pub fn send_test(&self, title: &str, body: &str) {
        self.emit(title, body, true);
    }

    fn emit(&self, title: &str, body: &str, force: bool) {
        let app = self.inner.app.clone();
        let enabled = force || self.inner.enabled.load(Ordering::Relaxed);
        if !enabled {
            return;
        }
        play_alert_sound();
        let title = title.to_string();
        let body = body.to_string();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = app
                .notification()
                .builder()
                .title(&title)
                .body(&body)
                .show()
            {
                tracing::warn!("failed to send notification: {error}");
            }
        });
    }
}

/// Plays the macOS Glass alert sound directly. Banner-attached sounds are
/// suppressed whenever the app is frontmost, so the sound is emitted here to
/// guarantee exactly one audible cue per notification, focused or not.
fn play_alert_sound() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_prime_without_firing() {
        let mut latch = None;
        assert!(!step_rule(&mut latch, Some(true), Some(false)));
        assert_eq!(latch, Some(true));
        assert!(!step_rule(&mut latch, Some(true), Some(false)));
        assert_eq!(latch, Some(true));
    }

    #[test]
    fn unlatched_rule_fires_on_false_to_true_edge() {
        let mut latch = None;
        assert!(!step_rule(&mut latch, Some(false), Some(true)));
        assert_eq!(latch, Some(false));
        assert!(step_rule(&mut latch, Some(true), Some(true)));
        assert_eq!(latch, Some(true));
    }

    #[test]
    fn latched_rule_only_clears_via_clears_condition() {
        let mut latch = Some(true);
        assert!(!step_rule(&mut latch, Some(false), Some(false)));
        assert_eq!(latch, Some(true), "hysteresis: no unlatch without clears");
        assert!(!step_rule(&mut latch, Some(false), Some(true)));
        assert_eq!(latch, Some(false));
    }

    #[test]
    fn no_data_rounds_leave_the_latch_untouched() {
        let mut latch = Some(true);
        assert!(!step_rule(&mut latch, None, None));
        assert_eq!(latch, Some(true));
        let mut latch = Some(false);
        assert!(!step_rule(&mut latch, None, None));
        assert_eq!(latch, Some(false));
    }

    #[test]
    fn from_bms_reads_cells_and_protection() {
        let mut snapshot = BatterySnapshot::new("bms".into(), "test".into());
        snapshot.soc = 55;
        snapshot.current = -12.5;
        snapshot.temperatures = vec![22.0, 31.0, 28.0];
        snapshot.protection_status = 0;
        let reading = Reading::from_bms(&snapshot);
        assert_eq!(reading.soc, Some(55.0));
        assert_eq!(reading.current_a, Some(-12.5));
        assert_eq!(reading.max_temp_c, Some(31.0));
        assert_eq!(reading.protection, Some(0));

        snapshot.protection_status = 0b10;
        let reading = Reading::from_bms(&snapshot);
        assert_eq!(reading.protection, Some(0b10));
    }

    #[test]
    fn empty_snapshot_yields_no_cell_temperature() {
        let snapshot = BatterySnapshot::new("bms".into(), "test".into());
        let reading = Reading::from_bms(&snapshot);
        assert_eq!(reading.max_temp_c, None);
    }
}