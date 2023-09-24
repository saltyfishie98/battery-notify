mod app_state;
mod battery;
mod helper;
mod watcher;

use app_state::{AppState, AppStateConfigs};
use std::sync::{Arc, Mutex};

const BATTERY_ID: u32 = 0;

const UNPLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Bottle.wav");

const PLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Blow.wav");

const LOW_BATT_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Funk.wav");

#[cfg(not(debug_assertions))]
async fn run_watchers() {
    let battery_state = Arc::new(Mutex::new(AppState::new(AppStateConfigs::default())));
    let (status_rx, status_watch) = make_status_watcher(battery_state.clone());
    let percent_watch = make_percent_watcher(battery_state, status_rx);
    let (_, _) = tokio::join!(percent_watch, status_watch);
}

#[cfg(debug_assertions)]
async fn run_watchers() {
    let battery_state = Arc::new(Mutex::new(AppState::new(AppStateConfigs { min: 60 })));

    let (status_rx, status_watch) = watcher::make_status_watcher(battery_state.clone());
    let percent_watch = watcher::make_percent_watcher(battery_state, status_rx);

    // let (_, _, _) = tokio::join!(percent_watch, status_watch, battery_state.log_status());
    let (_, _) = tokio::join!(percent_watch, status_watch);
}

#[tokio::main]
async fn main() {
    helper::setup_logging();
    run_watchers().await;
}
