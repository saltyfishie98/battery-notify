use notify::{RecursiveMode, Watcher};
use notify_rust::{Hint, Notification};
use rodio::Source;
use std::{
    future::Future,
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

use crate::battery;
use crate::helper;
use crate::AppState;

use crate::{BATTERY_ID, LOW_BATT_SOUND, PLUG_SOUND, UNPLUG_SOUND};

async fn play_sound(byte_data: &'static [u8], amplification: f32) {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();
    let cursor = Cursor::new(byte_data);
    let sound = rodio::Decoder::new(cursor).unwrap().amplify(amplification);

    sink.append(sound);
    sink.sleep_until_end();
}

pub async fn make_percent_watcher(
    state: Arc<Mutex<AppState>>,
    status_recv: watch::Receiver<battery::ChargeStatus>,
) -> notify::Result<()> {
    log::debug!("percent watcher started!");

    let battery_percent_file = battery::percent_path(BATTERY_ID);
    let percent_path = Path::new(battery_percent_file.as_str());
    let done = Arc::new(Mutex::new(false));

    let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;
    file_watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

    while let Some(res) = file_watcher_rx.recv().await {
        match res {
            Ok(_) => {
                let data = state.lock().unwrap();
                let percent_mutex = data.battery_state.clone();

                let percent = battery::Battery::get_live_percent(BATTERY_ID).unwrap();
                let mut battery_state = percent_mutex.lock().unwrap();
                let is_charging = battery_state.status == battery::ChargeStatus::Charging;
                let low_battery = percent <= data.min_battery_percent;

                let mut status_recv_1 = status_recv.clone();
                let done_notif = done.clone();
                let is_done = *done.lock().unwrap();

                log::debug!("battery percent update: {percent}%");

                if low_battery && !is_charging && !is_done {
                    let start_percent = data.start_charge_percent;
                    let duration =
                        chrono::Local::now().signed_duration_since(data.start_charge_time);

                    let seconds = duration.num_seconds() % 60;
                    let minutes = (duration.num_seconds() / 60) % 60;
                    let hours = (duration.num_seconds() / 60) / 60;
                    let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

                    log::debug!("low battery notification @ {percent}%");
                    tokio::spawn(play_sound(LOW_BATT_SOUND, 5.0));

                    {
                        let mut has_done = done_notif.lock().unwrap();
                        *has_done = true;
                    }

                    tokio::spawn(async move {
                        match Notification::new()
                            .summary(&format!("{}", helper::prog_name().unwrap()))
                            .body(&format!(
                                "battery charge is low!\nduration from {}% : T-{}",
                                start_percent, duration_str
                            ))
                            .hint(Hint::Transient(true))
                            .urgency(notify_rust::Urgency::Critical)
                            .timeout(Duration::from_millis(0))
                            .show()
                        {
                            Ok(handle) => {
                                if let Err(e) = status_recv_1
                                    .wait_for(|status| *status == battery::ChargeStatus::Charging)
                                    .await
                                {
                                    log::error!("status receiver error: {:?}", e);
                                }
                                handle.close();
                                log::debug!("battery notification close!");

                                let mut has_done = done_notif.lock().unwrap();
                                *has_done = false;
                            }
                            Err(e) => {
                                log::error!("percent notification error: {:?}", e);
                            }
                        }

                        log::debug!("cleared low battery notification @ {percent}%");
                    });
                }

                battery_state.percent = percent;
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}

pub fn make_status_watcher(
    state: Arc<Mutex<AppState>>,
) -> (
    watch::Receiver<battery::ChargeStatus>,
    impl Future<Output = notify::Result<()>>,
) {
    let (tx, rx) = watch::channel(battery::ChargeStatus::Unknown);

    (rx, async move {
        log::debug!("status watcher started!");

        let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;

        let battery_status_file = battery::status_path(BATTERY_ID);
        let status_path = Path::new(battery_status_file.as_str());

        file_watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

        while let Some(res) = file_watcher_rx.recv().await {
            match res {
                Ok(_) => {
                    let new_status = battery::Battery::get_live_status(BATTERY_ID).unwrap();

                    let mut data = state.lock().unwrap();
                    let status_mutex = data.battery_state.clone();

                    let mut battery_state = status_mutex.lock().unwrap();

                    if new_status != battery_state.status {
                        match new_status {
                            battery::ChargeStatus::Charging => {
                                if let Err(e) = tx.send(battery::ChargeStatus::Charging) {
                                    log::error!("status sender error: {}", e);
                                }

                                if let Err(e) = Notification::new()
                                    .summary(&format!("{}", helper::prog_name().unwrap()))
                                    .body("The battery has started charging!")
                                    .hint(Hint::Transient(true))
                                    .show()
                                {
                                    log::error!("status notification error: {:?}", e);
                                }

                                tokio::spawn(play_sound(PLUG_SOUND, 3.0));
                            }

                            battery::ChargeStatus::Discharging => {
                                if let Err(e) = tx.send(battery::ChargeStatus::NotCharging) {
                                    log::error!("status watch channel error: {}", e);
                                }

                                data.start_charge_time = chrono::Local::now();
                                data.start_charge_percent =
                                    battery::Battery::get_live_percent(BATTERY_ID).unwrap();

                                if let Err(e) = Notification::new()
                                    .summary(&format!("{}", helper::prog_name().unwrap()))
                                    .body("The battery has stopped charging!")
                                    .hint(Hint::Transient(true))
                                    .show()
                                {
                                    log::error!("status notification error: {:?}", e);
                                }

                                tokio::spawn(play_sound(UNPLUG_SOUND, 5.0));
                            }

                            battery::ChargeStatus::NotCharging => {
                                if let Err(e) = tx.send(battery::ChargeStatus::NotCharging) {
                                    log::error!("status watch channel error: {}", e);
                                }

                                if let Err(e) = Notification::new()
                                    .summary(&format!("{}", helper::prog_name().unwrap()))
                                    .body("The battery is fully charged!")
                                    .hint(Hint::Transient(true))
                                    .show()
                                {
                                    log::error!("status notification error: {:?}", e);
                                }
                            }

                            battery::ChargeStatus::Unknown => {
                                if let Err(e) = tx.send(battery::ChargeStatus::Charging) {
                                    log::error!("status watch channel error: {}", e);
                                }

                                if let Err(e) = Notification::new()
                                    .summary(&format!("{}", helper::prog_name().unwrap()))
                                    .body("The battery status is currently unknown!")
                                    .hint(Hint::Transient(true))
                                    .show()
                                {
                                    log::error!("status notification error: {:?}", e);
                                }
                            }
                        };
                    }

                    log::info!("battery status update: {:?}", new_status);
                    battery_state.status = new_status;
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }

        Ok(())
    })
}
