use crate::helper;
use crate::{battery, UserArgs};
use crate::{LOW_BATT_SOUND, PLUG_SOUND, UNPLUG_SOUND};
use notify::{RecursiveMode, Watcher};
use notify_rust::{Hint, Notification};
use rodio::Source;
use std::cell::RefCell;
use std::process::exit;
use std::{io::Cursor, path::Path, rc::Rc, time::Duration};
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

pub struct Notifier {
    battery_id: u32,
    min_battery_percent: u32,
    start_charge_percent: RefCell<u32>,
    start_charge_time: RefCell<chrono::DateTime<chrono::Local>>,
    battery_state: RefCell<battery::Battery>,
    status_recv_opt: RefCell<Option<watch::Receiver<battery::ChargeStatus>>>,
}

impl Notifier {
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

        Notifier {
            battery_id: config.battery_id,
            min_battery_percent: config.low_battery_percent,
            start_charge_percent: RefCell::new(
                battery::Battery::get_live_percent(config.battery_id).unwrap(),
            ),
            start_charge_time: RefCell::new(chrono::Local::now()),
            battery_state: RefCell::new(battery),
            status_recv_opt: RefCell::new(None),
        }
    }

    pub async fn make_percent_watcher(self: &Rc<Self>) -> notify::Result<()> {
        log::trace!("percent watcher started!");

        let battery_percent_file = battery::percent_path(self.battery_id);
        let percent_path = Path::new(battery_percent_file.as_str());
        let mut low_batt_notified = false;

        let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;
        file_watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

        while let Some(res) = file_watcher_rx.recv().await {
            match res {
                Ok(_) => {
                    let percent = battery::Battery::get_live_percent(self.battery_id).unwrap();
                    let low_battery = percent <= self.min_battery_percent;

                    log::trace!("battery percent update: {percent}%");

                    let is_charging;
                    let mut status_recv_1;
                    {
                        status_recv_1 = self.status_recv_opt.borrow_mut().clone().unwrap();
                        is_charging =
                            self.battery_state.borrow().status == battery::ChargeStatus::Charging;
                    }

                    if low_battery && !is_charging && !low_batt_notified {
                        log::debug!("low battery notification @ {percent}%");
                        low_batt_notified = true;

                        let duration = chrono::Local::now()
                            .signed_duration_since(*self.start_charge_time.borrow());
                        let seconds = duration.num_seconds() % 60;
                        let minutes = (duration.num_seconds() / 60) % 60;
                        let hours = (duration.num_seconds() / 60) / 60;
                        let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

                        match Notification::new()
                            .summary(&format!("{}", helper::prog_name().unwrap()))
                            .body(&format!(
                                "battery charge is low!\nduration from {}% : T-{}",
                                self.start_charge_percent.borrow(),
                                duration_str
                            ))
                            .hint(Hint::Transient(true))
                            .urgency(notify_rust::Urgency::Critical)
                            .timeout(Duration::from_millis(0))
                            .show()
                        {
                            Ok(handle) => {
                                play_embeded_sound(LOW_BATT_SOUND, 5.0).await;

                                if let Err(e) = status_recv_1
                                    .wait_for(|status| {
                                        *status == battery::ChargeStatus::Charging
                                            || *status == battery::ChargeStatus::NotCharging
                                    })
                                    .await
                                {
                                    log::error!("status receiver error: {:?}", e);
                                }
                                handle.close();
                                log::trace!("battery notification close!");

                                low_batt_notified = false;
                            }
                            Err(e) => {
                                log::error!("percent notification error: {:?}", e);
                            }
                        }

                        log::trace!("cleared low battery notification @ {percent}%");
                    }

                    let mut battery_state = self.battery_state.borrow_mut();
                    battery_state.percent = percent;
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }

        Ok(())
    }

    pub async fn make_status_watcher(self: &Rc<Self>) -> notify::Result<()> {
        log::trace!("status watcher started!");

        let (tx, rx) = watch::channel(battery::ChargeStatus::Unknown);

        {
            let mut local_rx = self.status_recv_opt.borrow_mut();
            *local_rx = Some(rx);
        }

        let batt_id = self.battery_id;

        let battery_status_file = battery::status_path(batt_id);
        let status_path = Path::new(battery_status_file.as_str());

        let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;
        file_watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

        while let Some(res) = file_watcher_rx.recv().await {
            match res {
                Ok(_) => {
                    let new_status = battery::Battery::get_live_status(batt_id).unwrap();

                    let old_status = self.battery_state.borrow().status;

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

                                {
                                    let mut start_charge_time = self.start_charge_time.borrow_mut();
                                    let mut start_charge_percent =
                                        self.start_charge_percent.borrow_mut();

                                    *start_charge_time = chrono::Local::now();
                                    *start_charge_percent =
                                        battery::Battery::get_live_percent(batt_id).unwrap();
                                }

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

                    let mut battery_state = self.battery_state.borrow_mut();
                    battery_state.status = new_status;
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }

        Ok(())
    }
}
