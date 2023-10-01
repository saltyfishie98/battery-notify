mod battery;
mod helper;
mod notifier;

use clap::Parser;
use notifier::Notifier;
use std::rc::Rc;

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
    let watcher_state = Rc::new(RefCell::new(Notifier::new(args)));

    #[cfg(debug_assertions)]
    let notifier = Rc::new(Notifier::new(UserArgs {
        battery_id: batt_id,
        low_battery_percent: 100,
    }));

    let status_watch = notifier.make_status_watcher();
    let percent_watch = notifier.make_percent_watcher();

    let _ = tokio::join!(percent_watch, status_watch);
}
