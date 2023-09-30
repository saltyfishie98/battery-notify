use crate::helper;
use crate::{battery, UserArgs};
use crate::{LOW_BATT_SOUND, PLUG_SOUND, UNPLUG_SOUND};
use notify::{RecursiveMode, Watcher};
use notify_rust::{Hint, Notification};
use rodio::Source;
use std::cell::RefCell;
use std::process::exit;
use std::{future::Future, io::Cursor, path::Path, rc::Rc, sync::RwLock, time::Duration};
use tokio::sync::watch;

async fn play_embeded_sound(byte_data: &'static [u8], amplification: f32) {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();
    let cursor = Cursor::new(byte_data);
    let sound = rodio::Decoder::new(cursor).unwrap().amplify(amplification);

    sink.append(sound);
    sink.sleep_until_end();
}

trait GetOwned<T> {
    fn get_owned(self, index: usize) -> Option<T>;
}

impl<T> GetOwned<T> for Vec<T> {
    fn get_owned(mut self, index: usize) -> Option<T> {
        #[allow(clippy::manual_map)]
        match self.get(index) {
            Some(_) => Some(self.remove(index)),
            None => None,
        }
    }
}

pub struct WatcherState {
    pub battery_id: u32,
    pub battery_state: Rc<RwLock<battery::Battery>>,
    pub min_battery_percent: u32,
    pub start_charge_percent: u32,
    pub start_charge_time: chrono::DateTime<chrono::Local>,
}

impl WatcherState {
    pub fn new(config: UserArgs) -> Self {
        let battery = match battery::Batteries::default()
            .entry
            .get_owned(config.battery_id as usize)
        {
            Some(out) => out,
            None => {
                log::error!("BAT{} does not exist!", config.battery_id);
                exit(1)
            }
        };

        log::info!("Watching BAT{}", config.battery_id);

        WatcherState {
            battery_id: config.battery_id,
            battery_state: Rc::new(RwLock::new(battery)),
            min_battery_percent: config.low_battery_percent,
            start_charge_percent: battery::Battery::get_live_percent(config.battery_id).unwrap(),
            start_charge_time: chrono::Local::now(),
        }
    }
}

pub async fn make_percent_watcher(
    batt_id: u32,
    watcher_state_ptr: Rc<RefCell<WatcherState>>,
    status_recv: watch::Receiver<battery::ChargeStatus>,
) -> notify::Result<()> {
    log::trace!("percent watcher started!");

    let battery_percent_file = battery::percent_path(batt_id);
    let percent_path = Path::new(battery_percent_file.as_str());
    let done = Rc::new(RwLock::new(false));

    let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;
    file_watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

    while let Some(res) = file_watcher_rx.recv().await {
        match res {
            Ok(_) => {
                let percent = battery::Battery::get_live_percent(batt_id).unwrap();

                let data = watcher_state_ptr.clone();
                let low_battery = percent <= data.borrow().min_battery_percent;
                let start_percent = data.borrow().start_charge_percent;
                let start_charge_time = data.borrow().start_charge_time;
                let battery_state_ptr = data.borrow().battery_state.clone();

                let is_charging;
                {
                    let battery_state = battery_state_ptr.read().unwrap();
                    is_charging = battery_state.status == battery::ChargeStatus::Charging;
                }

                let mut status_recv_1 = status_recv.clone();
                let done_notif = done.clone();
                let is_done = *done.read().unwrap();

                log::trace!("battery percent update: {percent}%");

                if low_battery && !is_charging && !is_done {
                    let duration = chrono::Local::now().signed_duration_since(start_charge_time);

                    let seconds = duration.num_seconds() % 60;
                    let minutes = (duration.num_seconds() / 60) % 60;
                    let hours = (duration.num_seconds() / 60) / 60;
                    let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

                    log::debug!("low battery notification @ {percent}%");

                    {
                        let mut has_done = done_notif.write().unwrap();
                        *has_done = true;
                    }

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
                            play_embeded_sound(LOW_BATT_SOUND, 5.0).await;

                            if let Err(e) = status_recv_1
                                .wait_for(|status| *status == battery::ChargeStatus::Charging)
                                .await
                            {
                                log::error!("status receiver error: {:?}", e);
                            }
                            handle.close();
                            log::trace!("battery notification close!");

                            let mut has_done = done_notif.write().unwrap();
                            *has_done = false;
                        }
                        Err(e) => {
                            log::error!("percent notification error: {:?}", e);
                        }
                    }

                    log::trace!("cleared low battery notification @ {percent}%");
                }

                let mut battery_state = battery_state_ptr.write().unwrap();
                battery_state.percent = percent;
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}

pub fn make_status_watcher(
    batt_id: u32,
    watcher_state_ptr: Rc<RefCell<WatcherState>>,
) -> (
    watch::Receiver<battery::ChargeStatus>,
    impl Future<Output = notify::Result<()>>,
) {
    let (tx, rx) = watch::channel(battery::ChargeStatus::Unknown);

    (rx, async move {
        log::trace!("status watcher started!");

        let battery_status_file = battery::status_path(batt_id);
        let status_path = Path::new(battery_status_file.as_str());

        let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;
        file_watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

        while let Some(res) = file_watcher_rx.recv().await {
            match res {
                Ok(_) => {
                    let new_status = battery::Battery::get_live_status(batt_id).unwrap();

                    let data = watcher_state_ptr.clone();
                    let battery_state_ptr = data.borrow().battery_state.clone();

                    let old_status;
                    {
                        let battery_state = battery_state_ptr.read().unwrap();
                        old_status = battery_state.status;
                    }

                    if new_status != old_status {
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

                                play_embeded_sound(PLUG_SOUND, 3.0).await;
                            }

                            battery::ChargeStatus::Discharging => {
                                if let Err(e) = tx.send(battery::ChargeStatus::NotCharging) {
                                    log::error!("status watch channel error: {}", e);
                                }

                                data.borrow_mut().start_charge_time = chrono::Local::now();
                                data.borrow_mut().start_charge_percent =
                                    battery::Battery::get_live_percent(batt_id).unwrap();

                                if let Err(e) = Notification::new()
                                    .summary(&format!("{}", helper::prog_name().unwrap()))
                                    .body("The battery has stopped charging!")
                                    .hint(Hint::Transient(true))
                                    .show()
                                {
                                    log::error!("status notification error: {:?}", e);
                                }

                                play_embeded_sound(UNPLUG_SOUND, 5.0).await;
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

                    let mut battery_state = battery_state_ptr.write().unwrap();
                    battery_state.status = new_status;
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }

        Ok(())
    })
}
