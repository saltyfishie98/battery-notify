mod battery;
mod helper;
mod notifier;

use clap::Parser;
use notifier::Notifier;
use std::{process::exit, rc::Rc};

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

    #[cfg(not(debug_assertions))]
    let notifier = Rc::new(Notifier::new(&args));

    #[cfg(debug_assertions)]
    let notifier = Rc::new(Notifier::new(&UserArgs {
        battery_id: args.battery_id,
        low_battery_percent: 100,
    }));

    let status_watch = notifier.make_status_watcher();
    let percent_watch = notifier.make_percent_watcher();

    match battery::Battery::get_live_percent(args.battery_id) {
        Ok(p) => {
            let _ = tokio::join!(
                percent_watch,
                status_watch,
                notifier.low_battery_notification(p)
            );
        }
        Err(e) => {
            log::error!("{:?}", e);
            exit(1);
        }
    };
}
