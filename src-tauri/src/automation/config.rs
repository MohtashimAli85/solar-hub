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
    pub water_run_minutes: u32,
    /// The lowest the daytime loads may draw the battery when the afternoon
    /// sun can still fill it again.
    pub day_low_soc: f64,
    pub geyser_enabled: bool,
    pub geyser_low_watts: f64,
    pub geyser_high_watts: f64,
    pub geyser_daily_kwh: f64,
    pub geyser_tank_litres: f64,
    pub geyser_start_hour: u32,
    pub geyser_end_hour: u32,
    /// The ESP32 IR blaster's host name or IP; empty means reminders only.
    pub geyser_esp32_host: String,
    pub geyser_auto_control: bool,
    /// Days (0 = Monday) the geyser heats ahead of the hotplate.
    pub geyser_first_weekdays: Vec<u32>,
    /// A night colder than this puts the geyser ahead of the hotplate too.
    pub geyser_first_below_c: f64,
    pub hotplate_enabled: bool,
    /// The level a cooking session is planned at; the hotplate runs 100–2200 W.
    pub hotplate_watts: f64,
    pub hotplate_hours: f64,
    pub hotplate_start_hour: u32,
    pub hotplate_end_hour: u32,
    pub hotplate_every_days: u32,
    pub ev_enabled: bool,
    pub ev_battery_kwh: f64,
    pub ev_charger_watts: f64,
    pub ev_daily_use_pct: f64,
    pub ev_ble_name: String,
    pub ev_ble_mac: String,
    /// Allows switching the scooter on to read a live %, then off again.
    pub ev_power_cycle_reads: bool,
    /// One live read a day at `ev_check_minute` (minutes after midnight).
    pub ev_daily_check: bool,
    pub ev_check_minute: u32,
    /// Above this % no charge is needed, unless tomorrow can't charge it.
    pub ev_no_charge_above_pct: f64,
    pub ev_remind_minutes: u32,
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
            water_run_minutes: 30,
            day_low_soc: 80.0,
            geyser_enabled: true,
            geyser_low_watts: 800.0,
            geyser_high_watts: 1200.0,
            geyser_daily_kwh: 3.0,
            geyser_tank_litres: 50.0,
            geyser_start_hour: 10,
            geyser_end_hour: 16,
            geyser_esp32_host: String::new(),
            geyser_auto_control: false,
            geyser_first_weekdays: vec![4],
            geyser_first_below_c: 10.0,
            hotplate_enabled: true,
            hotplate_watts: 1800.0,
            hotplate_hours: 2.5,
            hotplate_start_hour: 10,
            hotplate_end_hour: 15,
            hotplate_every_days: 2,
            ev_enabled: true,
            ev_battery_kwh: 2.16,
            ev_charger_watts: 600.0,
            ev_daily_use_pct: 25.0,
            ev_ble_name: "YD64071540".into(),
            ev_ble_mac: String::new(),
            ev_power_cycle_reads: false,
            ev_daily_check: true,
            ev_check_minute: 630,
            ev_no_charge_above_pct: 80.0,
            ev_remind_minutes: 30,
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
        self.water_run_minutes = self.water_run_minutes.clamp(5, 240);
        self.day_low_soc = self.day_low_soc.clamp(self.min_soc_percent, 100.0);
        self.geyser_low_watts = self.geyser_low_watts.clamp(100.0, 5_000.0);
        self.geyser_high_watts = self.geyser_high_watts.clamp(100.0, 5_000.0);
        self.geyser_daily_kwh = self.geyser_daily_kwh.clamp(0.5, 20.0);
        self.geyser_tank_litres = self.geyser_tank_litres.clamp(10.0, 500.0);
        self.geyser_start_hour = self.geyser_start_hour.clamp(6, 17);
        self.geyser_end_hour = self.geyser_end_hour.clamp(self.geyser_start_hour + 1, 18);
        self.geyser_esp32_host = self.geyser_esp32_host.trim().to_string();
        self.geyser_auto_control &= !self.geyser_esp32_host.is_empty();
        self.geyser_first_weekdays.retain(|day| *day < 7);
        self.geyser_first_weekdays.sort_unstable();
        self.geyser_first_weekdays.dedup();
        self.geyser_first_below_c = self.geyser_first_below_c.clamp(-20.0, 40.0);
        self.hotplate_watts = self.hotplate_watts.clamp(100.0, 5_000.0);
        self.hotplate_hours = self.hotplate_hours.clamp(0.5, 6.0);
        self.hotplate_start_hour = self.hotplate_start_hour.clamp(6, 17);
        self.hotplate_end_hour = self.hotplate_end_hour.clamp(self.hotplate_start_hour + 1, 18);
        self.hotplate_every_days = self.hotplate_every_days.clamp(1, 7);
        self.ev_battery_kwh = self.ev_battery_kwh.clamp(0.2, 100.0);
        self.ev_charger_watts = self.ev_charger_watts.clamp(100.0, 10_000.0);
        self.ev_daily_use_pct = self.ev_daily_use_pct.clamp(1.0, 100.0);
        self.ev_ble_name = self.ev_ble_name.trim().to_string();
        self.ev_ble_mac = self.ev_ble_mac.trim().to_uppercase();
        self.ev_check_minute = self.ev_check_minute.clamp(360, 960);
        self.ev_no_charge_above_pct = self.ev_no_charge_above_pct.clamp(0.0, 100.0);
        self.ev_remind_minutes = self.ev_remind_minutes.clamp(5, 120);
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
