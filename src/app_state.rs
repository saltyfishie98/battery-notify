use crate::battery;
use crate::BATTERY_ID;
use std::sync::{Arc, Mutex};

pub struct AppStateConfigs {
    pub min: u32,
}

impl Default for AppStateConfigs {
    fn default() -> Self {
        Self { min: 20 }
    }
}

pub struct AppState {
    pub battery_state: Arc<Mutex<battery::Battery>>,
    pub min_battery_percent: u32,
    pub start_charge_percent: u32,
    pub start_charge_time: chrono::DateTime<chrono::Local>,
}

impl AppState {
    pub fn new(config: AppStateConfigs) -> Self {
        AppState {
            battery_state: Arc::new(Mutex::new(
                battery::Batteries::default()
                    .entry
                    .remove(BATTERY_ID as usize),
            )),
            min_battery_percent: config.min,
            start_charge_percent: battery::Battery::get_live_percent(BATTERY_ID).unwrap(),
            start_charge_time: chrono::Local::now(),
        }
    }
}
