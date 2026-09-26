use serde::{Deserialize, Serialize};

fn default_safety_margin_hours() -> f64 {
    0.5
}

fn default_day_pv_threshold_watts() -> f64 {
    300.0
}

fn default_day_discharge_threshold_a() -> f64 {
    1.0
}

fn default_notifications_enabled() -> bool {
    true
}

fn default_probe_required_samples() -> u32 {
    3
}

fn default_min_hold_minutes() -> u64 {
    30
}

fn default_deficit_tolerance_hours() -> f64 {
    2.0
}

fn default_high_soc_hold_percent() -> f64 {
    85.0
}

fn default_hold_failures_before_revert() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub check_interval_minutes: u64,
    pub window_start_hour: u32,
    pub window_end_hour: u32,
    pub min_soc_percent: f64,
    pub reserve_soc_percent: f64,
    pub capacity_ah: f64,
    pub target_hour: u32,
    #[serde(default = "default_safety_margin_hours")]
    pub safety_margin_hours: f64,
    #[serde(default = "default_day_pv_threshold_watts")]
    pub day_pv_threshold_watts: f64,
    #[serde(default = "default_day_discharge_threshold_a")]
    pub day_discharge_threshold_a: f64,
    #[serde(default = "default_notifications_enabled")]
    pub notifications_enabled: bool,
    #[serde(default = "default_probe_required_samples")]
    pub probe_required_samples: u32,
    #[serde(default = "default_min_hold_minutes")]
    pub min_hold_minutes: u64,
    #[serde(default = "default_deficit_tolerance_hours")]
    pub deficit_tolerance_hours: f64,
    #[serde(default = "default_high_soc_hold_percent")]
    pub high_soc_hold_percent: f64,
    #[serde(default = "default_hold_failures_before_revert")]
    pub hold_failures_before_revert: u32,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_minutes: 15,
            window_start_hour: 21,
            window_end_hour: 6,
            min_soc_percent: 20.0,
            reserve_soc_percent: 15.0,
            capacity_ah: 100.0,
            target_hour: 5,
            safety_margin_hours: default_safety_margin_hours(),
            day_pv_threshold_watts: default_day_pv_threshold_watts(),
            day_discharge_threshold_a: default_day_discharge_threshold_a(),
            notifications_enabled: default_notifications_enabled(),
            probe_required_samples: default_probe_required_samples(),
            min_hold_minutes: default_min_hold_minutes(),
            deficit_tolerance_hours: default_deficit_tolerance_hours(),
            high_soc_hold_percent: default_high_soc_hold_percent(),
            hold_failures_before_revert: default_hold_failures_before_revert(),
        }
    }
}

impl AutomationConfig {
    pub fn clamp_for_ui(&mut self) {
        self.check_interval_minutes = self.check_interval_minutes.clamp(1, 240);
        self.window_start_hour = self.window_start_hour.clamp(0, 23);
        self.window_end_hour = self.window_end_hour.clamp(0, 23);
        self.min_soc_percent = self.min_soc_percent.clamp(0.0, 100.0);
        self.reserve_soc_percent = self.reserve_soc_percent.clamp(0.0, 100.0);
        self.capacity_ah = self.capacity_ah.clamp(1.0, 10_000.0);
        self.target_hour = self.target_hour.clamp(0, 23);
        self.safety_margin_hours = self.safety_margin_hours.clamp(0.0, 24.0);
        self.day_pv_threshold_watts = self.day_pv_threshold_watts.clamp(0.0, 50_000.0);
        self.day_discharge_threshold_a = self.day_discharge_threshold_a.clamp(0.0, 10_000.0);
        self.probe_required_samples = self.probe_required_samples.clamp(1, 12);
        self.min_hold_minutes = self.min_hold_minutes.clamp(0, 240);
        self.deficit_tolerance_hours = self.deficit_tolerance_hours.clamp(0.0, 12.0);
        self.high_soc_hold_percent = self.high_soc_hold_percent.clamp(0.0, 100.0);
        self.hold_failures_before_revert = self.hold_failures_before_revert.clamp(1, 10);
    }
}

/// A night window where `start == end` means "entire day"; otherwise the range
/// is allowed to wrap past midnight (start 18, end 8 covers 18:00–08:00).
pub fn hour_in_window(hour: u32, start_hour: u32, end_hour: u32) -> bool {
    if start_hour == end_hour {
        return true;
    }
    if start_hour < end_hour {
        hour >= start_hour && hour < end_hour
    } else {
        hour >= start_hour || hour < end_hour
    }
}

pub fn hours_until(hour: u32, target_hour: u32) -> f64 {
    if target_hour > hour {
        (target_hour - hour) as f64
    } else {
        (24 - hour + target_hour) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_matches_expected_semantics() {
        assert!(hour_in_window(0, 18, 8));
        assert!(hour_in_window(7, 18, 8));
        assert!(!hour_in_window(9, 18, 8));
        assert!(hour_in_window(20, 18, 8));
        assert!(hour_in_window(12, 0, 24));
        assert!(hour_in_window(18, 18, 18));
    }

    #[test]
    fn hours_until_wraps_past_midnight() {
        assert_eq!(hours_until(14, 17), 3.0);
        assert_eq!(hours_until(17, 17), 24.0);
        assert_eq!(hours_until(22, 17), 19.0);
        assert_eq!(hours_until(0, 17), 17.0);
    }

    #[test]
    fn defaults_match_nine_pm_to_five_am() {
        let config = AutomationConfig::default();
        assert_eq!(config.window_start_hour, 21);
        assert_eq!(config.window_end_hour, 6);
        assert_eq!(config.target_hour, 5);
        assert_eq!(config.safety_margin_hours, 0.5);
        assert_eq!(config.day_pv_threshold_watts, 300.0);
        assert_eq!(config.day_discharge_threshold_a, 1.0);
        assert!(config.notifications_enabled);
        assert_eq!(config.probe_required_samples, 3);
        assert_eq!(config.min_hold_minutes, 30);
        assert_eq!(config.deficit_tolerance_hours, 2.0);
        assert_eq!(config.high_soc_hold_percent, 85.0);
        assert_eq!(config.hold_failures_before_revert, 2);
    }

    #[test]
    fn config_saved_before_safety_margin_still_deserializes() {
        let legacy = serde_json::json!({
            "enabled": true,
            "check_interval_minutes": 15,
            "window_start_hour": 21,
            "window_end_hour": 6,
            "min_soc_percent": 20.0,
            "reserve_soc_percent": 15.0,
            "capacity_ah": 100.0,
            "target_hour": 5
        });
        let config: AutomationConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(config.safety_margin_hours, 0.5);
        assert_eq!(config.day_pv_threshold_watts, 300.0);
        assert_eq!(config.day_discharge_threshold_a, 1.0);
        assert!(config.notifications_enabled);
        assert_eq!(config.probe_required_samples, 3);
        assert_eq!(config.min_hold_minutes, 30);
        assert_eq!(config.deficit_tolerance_hours, 2.0);
        assert_eq!(config.high_soc_hold_percent, 85.0);
        assert_eq!(config.hold_failures_before_revert, 2);
    }

    #[test]
    fn clamp_bounds_safety_margin() {
        let mut config = AutomationConfig {
            safety_margin_hours: 999.0,
            ..AutomationConfig::default()
        };
        config.clamp_for_ui();
        assert_eq!(config.safety_margin_hours, 24.0);
        config.safety_margin_hours = -1.0;
        config.clamp_for_ui();
        assert_eq!(config.safety_margin_hours, 0.0);
    }

    #[test]
    fn clamp_bounds_daytime_thresholds() {
        let mut config = AutomationConfig {
            day_pv_threshold_watts: -50.0,
            day_discharge_threshold_a: -1.0,
            ..AutomationConfig::default()
        };
        config.clamp_for_ui();
        assert_eq!(config.day_pv_threshold_watts, 0.0);
        assert_eq!(config.day_discharge_threshold_a, 0.0);
        config.day_pv_threshold_watts = 1e9;
        config.day_discharge_threshold_a = 1e9;
        config.clamp_for_ui();
        assert_eq!(config.day_pv_threshold_watts, 50_000.0);
        assert_eq!(config.day_discharge_threshold_a, 10_000.0);
    }
}