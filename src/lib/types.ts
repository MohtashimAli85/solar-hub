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
  smart_load_drain_amps: number;
  smart_load_drain_minutes: number;
  notifications_enabled: boolean;
  water_reminder_enabled: boolean;
  water_motor_watts: number;
  water_start_hour: number;
  water_end_hour: number;
  water_full_soc: number;
  water_cloudy_hour: number;
  water_cloudy_soc: number;
  water_repeat_minutes: number;
  water_run_minutes: number;
  day_low_soc: number;
  geyser_enabled: boolean;
  geyser_low_watts: number;
  geyser_high_watts: number;
  geyser_daily_kwh: number;
  geyser_tank_litres: number;
  geyser_start_hour: number;
  geyser_end_hour: number;
  geyser_esp32_host: string;
  geyser_auto_control: boolean;
  geyser_first_weekdays: number[];
  geyser_first_below_c: number;
  hotplate_enabled: boolean;
  hotplate_watts: number;
  hotplate_hours: number;
  hotplate_start_hour: number;
  hotplate_end_hour: number;
  hotplate_every_days: number;
  ev_enabled: boolean;
  ev_battery_kwh: number;
  ev_charger_watts: number;
  ev_daily_use_pct: number;
  ev_ble_name: string;
  ev_ble_mac: string;
  ev_power_cycle_reads: boolean;
  ev_daily_check: boolean;
  ev_check_minute: number;
  ev_no_charge_above_pct: number;
  ev_remind_minutes: number;
}

export type WaterPhase = "off" | "waiting" | "ready" | "done" | "missed";

export interface WaterStatus {
  phase: WaterPhase;
  note: string | null;
  spare_w: number | null;
  last_reminded_at: LocalDateTime | null;
}

export type LoadKind = "water" | "geyser" | "ev" | "hotplate";

export type LoadPhase = "off" | "waiting" | "ready" | "running" | "stop" | "done" | "skipped" | "ask";

export interface LoadRow {
  phase: LoadPhase;
  note: string | null;
  watts: number | null;
  from: LocalDateTime | null;
  until: LocalDateTime | null;
  running_w: number | null;
  running_since: LocalDateTime | null;
  last_reminded_at: LocalDateTime | null;
}

export interface GeyserStatus extends LoadRow {
  delivered_kwh: number;
  need_kwh: number;
  heated_at: LocalDateTime | null;
  auto: boolean;
}

export type SocBasis = "read" | "live" | "estimated" | "full" | "set";

export type EditState = "on" | "off" | "done";

export interface LoadEdit {
  load: LoadKind;
  state: EditState;
  watts: number | null;
  kwh: number | null;
  soc: number | null;
}

export type EvCharge = "yes" | "if_spare";

export interface EvStatus extends LoadRow {
  soc: number | null;
  soc_at: LocalDateTime | null;
  soc_basis: SocBasis | null;
  hours_to_full: number | null;
  decision: EvCharge | null;
  decision_reason: string | null;
  decision_model: string | null;
  reading: boolean;
  read_error: string | null;
  charge_check: string | null;
  charge_today: boolean | null;
  charge_rule: string | null;
  checked_at: LocalDateTime | null;
  check_failed: boolean;
}

export interface SunHour {
  at: LocalDateTime;
  sun_w: number;
  house_w: number;
  loads_w: number;
}

export interface HotplateStatus extends LoadRow {
  cooking_day: boolean;
  asking_w: number | null;
  asking_since: LocalDateTime | null;
}

export interface LoadSlot {
  kind: LoadKind;
  start: LocalDateTime;
  end: LocalDateTime;
  watts: number;
  partial: boolean;
  topped_up: boolean;
}

export type LoadsAiMode = "off" | "ai" | "fallback";

export interface LoadsAiStatus {
  mode: LoadsAiMode;
  at: LocalDateTime | null;
  reason: string | null;
  model: string | null;
  problems: string[];
}

export interface LoadsQuestion {
  text: string;
  at: LocalDateTime;
}

export interface LoadsStatus {
  geyser: GeyserStatus;
  ev: EvStatus;
  hotplate: HotplateStatus;
  water: LoadRow;
  geyser_first: string | null;
  question: LoadsQuestion | null;
  ai: LoadsAiStatus;
  sun_hours: SunHour[];
  tank_question: LocalDateTime | null;
  water_running_since: LocalDateTime | null;
  spare_kwh: number | null;
  sunset_target_soc: number | null;
  projected_sunset_soc: number | null;
  full_by: LocalDateTime | null;
  pv_ratio: number | null;
  plan: LoadSlot[];
  grid_now: boolean;
  grid_kwh: number | null;
  geyser_required: string | null;
}

export interface ScooterReading {
  soc: number | null;
  read_at: LocalDateTime;
  power_cycled: boolean;
  off_sent: boolean;
  state_before: string | null;
  state_on: string | null;
  state_after: string | null;
}

export type GeyserButton = "power" | "w800" | "w1200" | "w2000" | "temp_up" | "temp_down";

export interface GeyserLearnResult {
  button: string;
  ok: boolean;
  error: string | null;
  length: number | null;
}

export interface GeyserRemoteStatus {
  learned: Record<string, boolean>;
  learning: string | null;
  last_sent: string | null;
  uptime_s: number | null;
  rssi: number | null;
  last_learn: GeyserLearnResult | null;
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
  ai_model: string | null;
  reserve_soc: number | null;
  next_check_at: string | null;
  boost_until: string | null;
  boosts_tonight: number;
  history_nights: number;
  history_dir: string;
  water: WaterStatus;
  loads: LoadsStatus;
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

export interface PvPoint {
  at: LocalDateTime;
  watts: number;
}

export interface MeasuredPv {
  at: LocalDateTime;
  watts: number;
  held_back: boolean;
}

export interface WeatherSummary {
  next_day: DayOutlook | null;
  recent_avg_radiation_kwh_m2: number | null;
  recent_days: RecentDay[];
  tonight_min_temp_c: number | null;
  recent_nights_min_temp_c: number | null;
  expected_pv_kwh_next_day: number | null;
  hours_until_sunset: HourOutlook[];
  today_pv: PvPoint[];
  today_measured: MeasuredPv[];
  fetched_at: LocalDateTime | null;
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
  on: boolean | null;
  /** At night it goes on once the battery drain holds at or above this for trigger_minutes. 0 = never. */
  trigger_amps: number;
  trigger_minutes: number;
  /** The latest battery drain tonight. */
  drain_a: number | null;
  /** What the engine last set this window: true = on (heavy loads cut). */
  applied: boolean | null;
  /** Near morning with enough battery: off for the rest of the night. */
  released: boolean;
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
  model: string | null;
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
  has_groq_api_key: boolean;
}

export interface RemoteStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  pin: string;
  urls: string[];
  paired_devices: number;
  error: string | null;
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
  groq_api_key?: string;
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
export interface GridSplit {
  grid_kwh: number;
  meter1_kwh: number;
  meter2_kwh: number;
  unassigned_kwh: number;
}

export interface DayUnits extends GridSplit {
  date: string;
  house_kwh: number;
  solar_kwh: number;
  battery_kwh: number;
  grid_off_minutes: number;
  data_hours: number;
  standby_kwh: number;
}

export interface HourUnits {
  hour: number;
  grid_kwh: number;
  house_kwh: number;
  solar_kwh: number;
  battery_kwh: number;
}

export interface BillPeriod {
  start: string;
  end: string;
  days_total: number;
  days_with_data: number;
  so_far: GridSplit;
  standby_kwh: number;
  log_hours: number;
  elapsed_hours: number;
  projected: GridSplit | null;
}

export type MeterNumber = 1 | 2;

export interface MeterAssignment {
  from: string;
  to: string;
  meter: MeterNumber;
}

export interface MeterBalance {
  target_kwh: number;
  switch_to: MeterNumber | null;
  switch_at: string | null;
  meter1_kwh: number;
  meter2_kwh: number;
}

export interface MeterReading {
  at: string;
  meter1: number;
  meter2: number;
}

export interface BillRates {
  fpa_per_unit: number;
  qtr_per_unit: number;
  adjustment_per_unit: number;
}

export interface MeterBill {
  units: number;
  fpa_units: number;
  protected: boolean;
  energy: number;
  fixed: number;
  surcharges: number;
  taxes: number;
  fpa: number;
  total: number;
}

export interface BillEstimate {
  start: string;
  end: string;
  basis: "plan" | "pace";
  meter1: MeterBill;
  meter2: MeterBill;
  total: number;
}

export interface PeriodSavings {
  start: string;
  end: string;
  partial: boolean;
  billed: boolean;
  coverage: number;
  units: number;
  units_without: number;
  bill: number;
  bill_without: number;
  saved: number;
}

export interface BilledUnits {
  start: string;
  meter1: number;
  meter2: number;
}

export interface MetersNow {
  start: MeterReading;
  latest: MeterReading;
  counted_kwh: number;
  factor: number;
  meter1_kwh: number;
  meter2_kwh: number;
}

export interface EnergySummary {
  configured: boolean;
  today: DayUnits | null;
  today_hourly: HourUnits[];
  yesterday: DayUnits | null;
  recent_days: DayUnits[];
  bill: BillPeriod | null;
  previous_bill: BillPeriod | null;
  balance: MeterBalance | null;
  meters: MetersNow | null;
  readings: MeterReading[];
  opening_reading: MeterReading | null;
  latest_reading: MeterReading | null;
  previous_billed: BilledUnits | null;
  bill_rates: BillRates;
  expected_bill: BillEstimate | null;
  savings: PeriodSavings[];
  bill_reading_day: number;
  bill_reading_time: string;
  standby_w: number;
  active_meter: MeterNumber | null;
  active_since: string | null;
  assignments: MeterAssignment[];
  updated_at: string | null;
  error: string | null;
  backfill: { done: number; total: number } | null;
  records_dir: string;
}
