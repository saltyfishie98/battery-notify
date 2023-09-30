mod battery;
mod helper;
mod watcher;

use clap::Parser;
use std::{rc::Rc, sync::RwLock};
use watcher::WatcherState;

const UNPLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Bottle.wav");

const PLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Blow.wav");

const LOW_BATT_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Funk.wav");

#[derive(Parser, Debug)]
pub struct UserArgs {
    #[arg(short = 'b', long = "batt", default_value_t = 0)]
    pub battery_id: u32,

    #[arg(short = 'l', long = "low", default_value_t = 20)]
    pub low_battery_percent: u32,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    helper::setup_logging();
    let args = UserArgs::parse();

    let batt_id = args.battery_id;

    #[cfg(not(debug_assertions))]
    let battery_state = Rc::new(RwLock::new(WatcherState::new(args)));

    #[cfg(debug_assertions)]
    let battery_state = Rc::new(RwLock::new(WatcherState::new(UserArgs {
        battery_id: batt_id,
        low_battery_percent: 100,
    })));

    let (status_rx, status_watch) = watcher::make_status_watcher(batt_id, battery_state.clone());
    let percent_watch = watcher::make_percent_watcher(batt_id, battery_state, status_rx);

    let (_, _) = tokio::join!(percent_watch, status_watch);
}
