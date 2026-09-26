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
  dry_run: boolean;
  check_interval_minutes: number;
  min_soc_percent: number;
  capacity_ah: number;
  sunrise_buffer_hours: number;
  morning_window_hours: number;
  morning_charge_threshold_a: number;
  pv_array_watts: number;
  notifications_enabled: boolean;
}

export interface AutomationLastEvent {
  timestamp: string;
  message: string;
}

export type AutomationPhase =
  | "idle"
  | "night_deciding"
  | "night_verifying"
  | "night_holding"
  | "paused"
  | "morning"
  | "blocked";

export interface AutomationStatus {
  enabled: boolean;
  dry_run: boolean;
  phase: AutomationPhase;
  mode_name: string | null;
  last_event: AutomationLastEvent | null;
  sunrise: string | null;
  sunset: string | null;
  blocked_reason: string | null;
  ai_reason: string | null;
}

export interface AppSettings {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  saved_ble_device_id: string;
  saved_ble_device_name: string;
  latitude: number | null;
  longitude: number | null;
  has_gemini_api_key: boolean;
}

export interface SolarSettingsInput {
  user_id: string;
  station_id: string;
  device_id: string;
  time_zone: string;
  password?: string;
  latitude?: number | null;
  longitude?: number | null;
  gemini_api_key?: string;
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