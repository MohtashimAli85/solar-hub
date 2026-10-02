use std::time::Duration;

use chrono::Utc;
use tauri::AppHandle;

use crate::automation::AutomationState;
use crate::battery::BatteryState;
use crate::energy::EnergyState;
use crate::events;
use crate::inverter::client::SolarClient;

/// Tokio timers stop while the Mac sleeps, so a long wall-clock jump between
/// two short probes is how a wake-up shows itself.
const PROBE_EVERY: Duration = Duration::from_secs(20);
const ASLEEP_GAP_SECS: i64 = 60;
const SETTLE: Duration = Duration::from_secs(15);

pub fn spawn(app: AppHandle, automation: AutomationState, energy: EnergyState, battery: BatteryState, client: SolarClient) {
    tauri::async_runtime::spawn(async move {
        let mut last = Utc::now();
        loop {
            tokio::time::sleep(PROBE_EVERY).await;
            let gap = (Utc::now() - last).num_seconds();
            last = Utc::now();
            if gap < PROBE_EVERY.as_secs() as i64 + ASLEEP_GAP_SECS {
                continue;
            }
            tracing::info!("woke after about {} min asleep, refreshing", gap / 60);
            client.forget_energy_flow().await;
            tokio::time::sleep(SETTLE).await;
            crate::battery::manager::on_resume(battery.clone(), app.clone()).await;
            automation.wake.notify_one();
            energy.wake.notify_one();
            events::emit(&app, events::SYSTEM_RESUMED, gap);
            last = Utc::now();
        }
    });
}
