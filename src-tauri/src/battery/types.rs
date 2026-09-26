use serde::Serialize;

use super::jbd_protocol::BasicStatus;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub rssi: Option<i16>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub reconnecting: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatterySnapshot {
    pub device_id: String,
    pub device_name: String,
    pub voltage: f64,
    pub current: f64,
    pub remaining_capacity: f64,
    pub rated_capacity: f64,
    pub cycles: u16,
    pub production_date: String,
    pub balance_status: u16,
    pub protection_status: u16,
    pub soc: u8,
    pub fet_status: u8,
    pub temperatures: Vec<f64>,
    pub cell_voltages: Vec<f64>,
    pub time_to_15_hours: Option<f64>,
}

impl BatterySnapshot {
    pub fn new(device_id: String, device_name: String) -> Self {
        Self {
            device_id,
            device_name,
            voltage: 0.0,
            current: 0.0,
            remaining_capacity: 0.0,
            rated_capacity: 0.0,
            cycles: 0,
            production_date: String::new(),
            balance_status: 0,
            protection_status: 0,
            soc: 0,
            fet_status: 0,
            temperatures: Vec::new(),
            cell_voltages: Vec::new(),
            time_to_15_hours: None,
        }
    }

    pub fn update_from(&mut self, status: &BasicStatus, cell_voltages: &[f64]) {
        self.voltage = status.voltage;
        self.current = status.current;
        self.remaining_capacity = status.remaining_capacity;
        self.rated_capacity = status.rated_capacity;
        self.cycles = status.cycles;
        self.production_date = status.production_date.clone();
        self.balance_status = status.balance_status;
        self.protection_status = status.protection_status;
        self.soc = status.soc;
        self.fet_status = status.fet_status;
        self.temperatures = status.temperatures.clone();
        if !cell_voltages.is_empty() {
            self.cell_voltages = cell_voltages.to_vec();
        }
        let target_capacity = status.rated_capacity * 0.15;
        let usable_capacity = status.remaining_capacity - target_capacity;
        self.time_to_15_hours = if status.current < 0.0 && usable_capacity > 0.0 {
            Some(usable_capacity / status.current.abs())
        } else {
            None
        };
    }
}