use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub dry_run: bool,
    pub check_interval_minutes: u64,
    pub min_soc_percent: f64,
    pub capacity_ah: f64,
    pub sunrise_buffer_hours: f64,
    pub night_start_hour: u32,
    pub pv_array_watts: f64,
    /// Night-time load (as battery-side amps) that triggers a short oven
    /// boost from the battery. 0 turns it off.
    pub oven_boost_amps: f64,
    /// Night-time battery drain (amps) that, held for
    /// `smart_load_drain_minutes`, enables smart load. 0 turns it off.
    pub smart_load_drain_amps: f64,
    pub smart_load_drain_minutes: u32,
    pub notifications_enabled: bool,
    pub water_reminder_enabled: bool,
    pub water_motor_watts: f64,
    pub water_start_hour: u32,
    pub water_end_hour: u32,
    pub water_full_soc: f64,
    pub water_cloudy_hour: u32,
    pub water_cloudy_soc: f64,
    pub water_repeat_minutes: u32,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: true,
            check_interval_minutes: 15,
            min_soc_percent: 20.0,
            capacity_ah: 100.0,
            sunrise_buffer_hours: 1.0,
            night_start_hour: 21,
            pv_array_watts: 0.0,
            oven_boost_amps: 50.0,
            smart_load_drain_amps: 25.0,
            smart_load_drain_minutes: 10,
            notifications_enabled: true,
            water_reminder_enabled: true,
            water_motor_watts: 1000.0,
            water_start_hour: 11,
            water_end_hour: 15,
            water_full_soc: 100.0,
            water_cloudy_hour: 14,
            water_cloudy_soc: 80.0,
            water_repeat_minutes: 15,
        }
    }
}

impl AutomationConfig {
    pub fn clamp_for_ui(&mut self) {
        self.check_interval_minutes = self.check_interval_minutes.clamp(1, 240);
        self.min_soc_percent = self.min_soc_percent.clamp(0.0, 100.0);
        self.capacity_ah = self.capacity_ah.clamp(1.0, 10_000.0);
        self.sunrise_buffer_hours = self.sunrise_buffer_hours.clamp(0.0, 6.0);
        self.night_start_hour = self.night_start_hour.clamp(17, 23);
        self.pv_array_watts = self.pv_array_watts.clamp(0.0, 100_000.0);
        self.oven_boost_amps = self.oven_boost_amps.clamp(0.0, 300.0);
        self.smart_load_drain_amps = self.smart_load_drain_amps.clamp(0.0, 300.0);
        self.smart_load_drain_minutes = self.smart_load_drain_minutes.clamp(1, 60);
        self.water_motor_watts = self.water_motor_watts.clamp(100.0, 10_000.0);
        self.water_start_hour = self.water_start_hour.clamp(6, 17);
        self.water_end_hour = self.water_end_hour.clamp(self.water_start_hour + 1, 18);
        self.water_cloudy_hour = self.water_cloudy_hour.clamp(self.water_start_hour, self.water_end_hour - 1);
        self.water_full_soc = self.water_full_soc.clamp(0.0, 100.0);
        self.water_cloudy_soc = self.water_cloudy_soc.clamp(0.0, 100.0);
        self.water_repeat_minutes = self.water_repeat_minutes.clamp(5, 120);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_dry_run_and_disabled() {
        let config = AutomationConfig::default();
        assert!(!config.enabled);
        assert!(config.dry_run);
        assert_eq!(config.check_interval_minutes, 15);
        assert_eq!(config.min_soc_percent, 20.0);
        assert_eq!(config.night_start_hour, 21);
    }

    #[test]
    fn legacy_config_without_new_fields_still_deserializes() {
        let legacy = serde_json::json!({
            "enabled": true,
            "check_interval_minutes": 10,
            "morning_window_hours": 3.0,
            "morning_charge_threshold_a": 15.0
        });
        let config: AutomationConfig = serde_json::from_value(legacy).unwrap();
        assert!(config.enabled);
        assert_eq!(config.check_interval_minutes, 10);
        assert!(config.dry_run, "dry_run defaults true even from a legacy save");
        assert_eq!(config.night_start_hour, 21);
    }

    #[test]
    fn clamp_bounds_night_start() {
        let mut config = AutomationConfig {
            night_start_hour: 2,
            ..AutomationConfig::default()
        };
        config.clamp_for_ui();
        assert_eq!(config.night_start_hour, 17);
    }
}
