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
  energy_flow_stale?: boolean;
  grid?: GridStatus | null;
}

export type GridBasis = "battery_draw_in_solar_mode" | "ac_input" | "load_without_battery" | "unknown";

export interface GridStatus {
  on: boolean | null;
  basis: GridBasis;
  voltage: number | null;
  power_w: number | null;
}

export interface AutomationConfig {
  enabled: boolean;
  dry_run: boolean;
  check_interval_minutes: number;
  min_soc_percent: number;
  capacity_ah: number;
  sunrise_buffer_hours: number;
  night_start_hour: number;
  pv_array_watts: number;
  oven_boost_amps: number;
  notifications_enabled: boolean;
}

export interface AutomationLastEvent {
  timestamp: string;
  message: string;
}

export type AutomationPhase =
  | "idle"
  | "day"
  | "night_deciding"
  | "night_verifying"
  | "night_on_battery"
  | "night_reserve"
  | "paused"
  | "blocked";

export interface AutomationStatus {
  enabled: boolean;
  dry_run: boolean;
  phase: AutomationPhase;
  mode_name: string | null;
  effective_mode: string | null;
  last_event: AutomationLastEvent | null;
  sunrise: string | null;
  sunset: string | null;
  blocked_reason: string | null;
  ai_reason: string | null;
  reserve_soc: number | null;
  next_check_at: string | null;
  boost_until: string | null;
  boosts_tonight: number;
  history_nights: number;
  history_dir: string;
}

/** Local wall-clock time without a zone, e.g. "2026-09-26T21:05:00". */
export type LocalDateTime = string;

export interface SocPoint {
  at: LocalDateTime;
  soc: number;
}

export interface HourLoad {
  hour: number;
  load_w: number;
  nights: number;
}

export interface Outage {
  start: LocalDateTime;
  minutes: number;
  ongoing: boolean;
}

export interface DayOutlook {
  date: string;
  radiation_kwh_m2: number | null;
  sunshine_h: number | null;
  cloud_pct: number | null;
  rain_prob_pct: number | null;
}

export interface RecentDay {
  date: string;
  radiation_kwh_m2: number | null;
  max_soc: number | null;
  full_at: string | null;
}

export interface HourOutlook {
  at: LocalDateTime;
  cloud_pct: number | null;
  radiation_w_m2: number | null;
}

export interface WeatherSummary {
  next_day: DayOutlook | null;
  recent_avg_radiation_kwh_m2: number | null;
  recent_days: RecentDay[];
  tonight_min_temp_c: number | null;
  recent_nights_min_temp_c: number | null;
  expected_pv_kwh_next_day: number | null;
  hours_until_sunset: HourOutlook[];
}

export type ProjectionMethod = "pv_array" | "pv_headroom" | "charge_scaling" | "constant_charge" | "after_sunset";

export interface DayProjection {
  soc_now: number;
  soc_at_sunset: number;
  full_at: LocalDateTime | null;
  hours_of_sun_left: number;
  ah_to_full: number;
  charge_a: number;
  max_charge_a: number | null;
  method: ProjectionMethod;
}

export interface AutomationInsights {
  updated_at: LocalDateTime | null;
  window: "day" | "night" | null;
  sunrise: LocalDateTime | null;
  sunset: LocalDateTime | null;
  night_start: LocalDateTime | null;
  next_sunrise: LocalDateTime | null;
  soc: number | null;
  simulated_soc: number | null;
  floor_soc: number;
  reserve_soc: number | null;
  trajectory: SocPoint[];
  trajectory_is_preview: boolean;
  trajectory_basis: "history" | "current_load" | null;
  actual_soc: SocPoint[];
  reserve_eta: LocalDateTime | null;
  backup_hours: number | null;
  routine: {
    nights_with_data: number;
    typical: HourLoad[];
    tonight: HourLoad[];
    quiet_by: number | null;
  };
  weather: WeatherSummary | null;
  outages: Outage[];
  day: DayProjection | null;
  records: {
    dir: string;
    samples_this_month: number;
    decisions_this_month: number;
  };
  grid: GridStatus | null;
  smart_load: SmartLoadInsight | null;
}

export interface SmartLoadInsight {
  season: "summer" | "winter";
  on: boolean | null;
  planned_on_at: LocalDateTime | null;
  night_on_at: LocalDateTime | null;
  done: boolean;
}

export interface AutomationDecision {
  at: LocalDateTime;
  window: "day" | "night";
  mode: string;
  reserve_soc: number | null;
  recheck_minutes: number | null;
  confidence: number | null;
  dry_run: boolean;
  applied: boolean;
  reason: string;
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