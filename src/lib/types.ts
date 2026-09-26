export interface DiscoveredDevice {
  id: string;
  name: string;
  rssi: number | null;
}

export interface SavedBleDevice {
  id: string;
  name: string;
}

export interface ConnectionStatus {
  connected: boolean;
  device_id: string | null;
  device_name: string | null;
  reconnecting: boolean;
}

export interface BatterySnapshot {
  device_id: string;
  device_name: string;
  voltage: number;
  current: number;
  remaining_capacity: number;
  rated_capacity: number;
  cycles: number;
  production_date: string;
  balance_status: number;
  protection_status: number;
  soc: number;
  fet_status: number;
  temperatures: number[];
  cell_voltages: number[];
  time_to_15_hours: number | null;
}

export interface InverterSettings {
  output_source_priority: string;
  output_source_priority_value: number | null;
  charger_source_priority: string;
  charger_source_priority_value: number | null;
  ac_input_range: number | null;
  battery_power_limiting: number | null;
  low_battery_cutoff_voltage: number | null;
  high_cutoff_voltage: number | null;
  low_dc_cutoff_soc: number | null;
  max_total_charge_current: number | null;
  max_utility_charge_current: number | null;
  smart_load: number | null;
}

export type InverterFields = Record<string, unknown>;

export interface InverterSnapshot {
  device_id: string;
  fields: InverterFields;
  groups: unknown[];
  firing_alarms: unknown[];
  settings: InverterSettings | null;
  pv_panel_flow?: unknown;
  grid_flow?: unknown;
  load_flow?: unknown;
}

export interface AutomationConfig {
  enabled: boolean;
  check_interval_minutes: number;
  window_start_hour: number;
  window_end_hour: number;
  min_soc_percent: number;
  reserve_soc_percent: number;
  capacity_ah: number;
  target_hour: number;
  safety_margin_hours: number;
  day_pv_threshold_watts: number;
  day_discharge_threshold_a: number;
  notifications_enabled: boolean;
  probe_required_samples: number;
  min_hold_minutes: number;
  deficit_tolerance_hours: number;
  high_soc_hold_percent: number;
  hold_failures_before_revert: number;
}

export interface AutomationLogEntry {
  timestamp: string;
  message: string;
}

export interface AutomationWarning {
  timestamp: string;
  message: string;
}

export type AutomationPhase =
  | "day"
  | "waiting"
  | "probing"
  | "holding"
  | "backed_off"
  | "user_override"
  | "low_soc";

export interface AutomationLiveStatus {
  soc: number | null;
  battery_current_a: number | null;
  pv_w: number | null;
  load_w: number | null;
  grid_on: boolean | null;
  smart_load: number | null;
}

export interface AutomationEstimate {
  discharge_a: number | null;
  runtime_h: number | null;
}

export interface AutomationStatus {
  enabled: boolean;
  last_check: string | null;
  current_mode_name: string | null;
  warning: AutomationWarning | null;
  automation_engaged: boolean;
  phase: AutomationPhase;
  next_check: string | null;
  live: AutomationLiveStatus;
  estimate: AutomationEstimate;
  verified: AutomationEstimate;
  required_h: number | null;
  data_source: "bms" | "inverter" | null;
  logs: AutomationLogEntry[];
}

export interface AppSettings {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  saved_ble_device_id: string;
  saved_ble_device_name: string;
}

export interface SolarSettingsInput {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  password?: string;
}

export interface DeviceDetails {
  is_online: boolean;
  rated_power: number | null;
  daily_produced_quantity: number | null;
  total_produced_quantity: number | null;
  state_dict: string | null;
  station_name: string | null;
  co2_emission_reduction: number | null;
  so2_emission_reduction: number | null;
  nox_emission_reduction: number | null;
}

export const OUTPUT_MODES: { value: string; label: string }[] = [
  { value: "0", label: "Solar" },
  { value: "1", label: "SBG" },
  { value: "2", label: "Utility" },
];

export const CHARGER_MODES: { value: string; label: string }[] = [
  { value: "0", label: "Solar" },
  { value: "1", label: "Solar or Utility" },
  { value: "2", label: "Solar only" },
];

export const TZ_PRESETS = [
  "Asia/Karachi",
  "UTC",
  "Asia/Calcutta",
  "Asia/Dubai",
  "Asia/Riyadh",
  "Asia/Kolkata",
];