use automation::AutomationState;
use battery::BatteryState;
use inverter::client::{SolarClient, SolarCredentials};
use inverter::InverterState;
use state::AppState;
use tauri::Manager;
use storage::{get_solar_password, load_app_settings, load_automation_config, load_energy_config, load_remote_config};
use tracing_appender::non_blocking::WorkerGuard;

pub mod automation;
pub mod battery;
pub mod energy;
pub mod events;
pub mod inverter;
pub mod notifications;
pub mod remote;
pub mod state;
pub mod storage;

/// Keeps the non-blocking file log writer alive for the life of the app —
/// dropping this stops flushing to `solar-hub.log`.
struct LogGuard(#[allow(dead_code)] WorkerGuard);

fn init_logging(log_dir: &std::path::Path) -> LogGuard {
    let _ = std::fs::create_dir_all(log_dir);
    let file_appender = tracing_appender::rolling::never(log_dir, "solar-hub.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .try_init();
    LogGuard(guard)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let log_dir = handle
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let log_guard = init_logging(&log_dir);
            tracing::info!("solar-hub starting, logging to {}", log_dir.display());
            app.manage(log_guard);

            let settings = load_app_settings(&handle);
            let password = get_solar_password()
                .ok()
                .flatten()
                .unwrap_or_default();
            let client = SolarClient::new(SolarCredentials {
                user_id: settings.user_id.clone(),
                password,
                station_id: settings.station_id.clone(),
                device_id: settings.device_id.clone(),
                time_zone: settings.time_zone.clone(),
            });

            let inverter = InverterState::new(client.clone());
            let battery = BatteryState::new();
            let automation_config = load_automation_config(&handle);
            let notifier = notifications::Notifier::new(handle.clone());
            notifier.set_enabled(automation_config.notifications_enabled);
            let records_root = handle
                .path()
                .document_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .join("Solar Hub");
            let history = automation::history::HistoryStore::new(records_root);
            let energy = energy::EnergyState::new(load_energy_config(&handle), history.clone());
            let automation = AutomationState::new(automation_config, history);

            app.manage(inverter.clone());
            app.manage(battery.clone());
            app.manage(automation.clone());
            app.manage(notifier.clone());
            app.manage(energy.clone());
            battery::manager::spawn_disconnect_watcher(battery.clone(), handle.clone());
            battery::manager::spawn_reconnect_loop(battery.clone(), handle.clone());
            app.manage(AppState {
                inverter,
                battery: battery.clone(),
                automation: automation.clone(),
            });

            let remote = remote::RemoteState::new(load_remote_config(&handle).unwrap_or_default());
            app.manage(remote.clone());
            let remote_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                if remote.enabled().await {
                    remote.start(&remote_app).await;
                }
            });

            energy::runner::spawn(energy, client.clone(), handle.clone());
            automation::runner::spawn(automation, client, battery, notifier, handle.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            battery::commands::scan_bms_devices,
            battery::commands::get_battery_state,
            battery::commands::get_battery_connection,
            battery::commands::connect_bms_device,
            battery::commands::disconnect_bms_device,
            battery::commands::reconnect_saved_bms,
            battery::commands::get_saved_bms_device,
            battery::commands::set_saved_bms_device,
            inverter::commands::get_inverter_snapshot,
            inverter::commands::get_inverter_settings,
            inverter::commands::set_output_priority,
            inverter::commands::set_charger_priority,
            inverter::commands::set_ac_input_range,
            inverter::commands::set_grid_feed_in,
            inverter::commands::set_smart_load,
            inverter::commands::set_low_battery_cutoff_voltage,
            inverter::commands::set_high_cutoff_voltage,
            inverter::commands::set_low_dc_cutoff_soc,
            inverter::commands::set_max_total_charge_current,
            inverter::commands::set_max_utility_charge_current,
            inverter::commands::get_device_details,
            inverter::commands::get_solar_settings,
            inverter::commands::update_solar_settings,
            automation::commands::get_automation_status,
            automation::commands::get_automation_config,
            automation::commands::update_automation_config,
            automation::commands::force_automation_check,
            automation::commands::get_automation_insights,
            automation::commands::get_automation_decisions,
            automation::commands::open_records_folder,
            automation::commands::send_test_notification,
            energy::commands::get_energy_summary,
            energy::commands::set_bill_reading,
            energy::commands::set_active_meter,
            energy::commands::set_standby_watts,
            remote::commands::get_remote_status,
            remote::commands::set_remote_enabled,
            remote::commands::regenerate_remote_pin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}