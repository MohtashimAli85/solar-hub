import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AutomationConfig,
  AutomationStatus,
  BatterySnapshot,
  ConnectionStatus,
  DeviceDetails,
  DiscoveredDevice,
  InverterSettings,
  InverterSnapshot,
  SavedBleDevice,
  SolarSettingsInput,
} from "./types";

// — Battery (BLE) —

export const scanBmsDevices = () => invoke<DiscoveredDevice[]>("scan_bms_devices");

export const getBatteryState = () => invoke<BatterySnapshot | null>("get_battery_state");

export const getBatteryConnection = () => invoke<ConnectionStatus>("get_battery_connection");

export const connectBmsDevice = (deviceId: string) =>
  invoke<void>("connect_bms_device", { deviceId });

export const disconnectBmsDevice = () => invoke<void>("disconnect_bms_device");

export const reconnectSavedBms = () => invoke<void>("reconnect_saved_bms");

export const getSavedBmsDevice = () => invoke<SavedBleDevice | null>("get_saved_bms_device");

export const setSavedBmsDevice = (deviceId: string, deviceName: string) =>
  invoke<void>("set_saved_bms_device", { deviceId, deviceName });

// — Inverter (Solar) —

export const getInverterSnapshot = () => invoke<InverterSnapshot>("get_inverter_snapshot");

export const getInverterSettings = () =>
  invoke<InverterSettings>("get_inverter_settings", { deviceId: null });

export const setOutputPriority = (mode: string) =>
  invoke<InverterSettings>("set_output_priority", { mode, deviceId: null });

export const setChargerPriority = (mode: string) =>
  invoke<InverterSettings>("set_charger_priority", { mode, deviceId: null });

export const setAcInputRange = (enabled: boolean) =>
  invoke<InverterSettings>("set_ac_input_range", { enabled, deviceId: null });

export const setGridFeedIn = (enabled: boolean) =>
  invoke<InverterSettings>("set_grid_feed_in", { enabled, deviceId: null });

export const setSmartLoad = (enabled: boolean) =>
  invoke<InverterSettings>("set_smart_load", { enabled, deviceId: null });

export const setLowBatteryCutoffVoltage = (value: number) =>
  invoke<InverterSettings>("set_low_battery_cutoff_voltage", { value, deviceId: null });

export const setHighCutoffVoltage = (value: number) =>
  invoke<InverterSettings>("set_high_cutoff_voltage", { value, deviceId: null });

export const setLowDcCutoffSoc = (value: number) =>
  invoke<InverterSettings>("set_low_dc_cutoff_soc", { value, deviceId: null });

export const setMaxTotalChargeCurrent = (value: number) =>
  invoke<InverterSettings>("set_max_total_charge_current", { value, deviceId: null });

export const setMaxUtilityChargeCurrent = (value: number) =>
  invoke<InverterSettings>("set_max_utility_charge_current", { value, deviceId: null });

export const getDeviceDetails = () =>
  invoke<DeviceDetails>("get_device_details", { deviceId: null });

export const getSolarSettings = () => invoke<AppSettings>("get_solar_settings");

export const updateSolarSettings = (input: SolarSettingsInput) =>
  invoke<AppSettings>("update_solar_settings", { input });

// — Automation —

export const getAutomationStatus = () => invoke<AutomationStatus>("get_automation_status");

export const getAutomationConfig = () =>
  invoke<AutomationConfig>("get_automation_config");

export const updateAutomationConfig = (config: AutomationConfig) =>
  invoke<AutomationConfig>("update_automation_config", { config });

export const forceAutomationCheck = () =>
  invoke<AutomationStatus>("force_automation_check");

export const dismissAutomationWarning = () =>
  invoke<AutomationStatus>("dismiss_automation_warning");

export const sendTestNotification = () => invoke<void>("send_test_notification");