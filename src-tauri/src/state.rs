use crate::automation::AutomationState;
use crate::battery::BatteryState;
use crate::inverter::InverterState;

#[derive(Clone)]
pub struct AppState {
    pub inverter: InverterState,
    pub battery: BatteryState,
    pub automation: AutomationState,
}