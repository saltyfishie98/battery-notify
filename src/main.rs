use notify::{Config, Event, PollWatcher, RecursiveMode, Watcher};
use notify_rust::{Hint, Notification};
use rodio::Source;
use std::{
    future::Future,
    io::Cursor,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::sync::watch;

mod battery;

struct StateConfigs {
    min: u32,
}

impl Default for StateConfigs {
    fn default() -> Self {
        Self { min: 20 }
    }
}

struct State {
    battery_state: Arc<Mutex<battery::Battery>>,
    min_battery_percent: u32,
    start_charge_percent: u32,
    start_charge_time: chrono::DateTime<chrono::Local>,
}

impl State {
    fn new(config: StateConfigs) -> Self {
        State {
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

async fn play_sound(byte_data: &'static [u8], amplification: f32) {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();
    let cursor = Cursor::new(byte_data);
    let sound = rodio::Decoder::new(cursor).unwrap().amplify(amplification);

    sink.append(sound);
    sink.sleep_until_end();
}

async fn make_percent_watcher(
    state: Arc<Mutex<State>>,
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

fn make_status_watcher(
    state: Arc<Mutex<State>>,
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

const UNPLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Bottle.wav");

const PLUG_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Blow.wav");

const LOW_BATT_SOUND: &[u8] =
    std::include_bytes!("/home/saltyfishie/.local/share/sounds/big_sur/Funk.wav");

const BATTERY_ID: u32 = 0;

#[tokio::main]
async fn main() {
    helper::setup_logging();
    run_watchers().await;
}

#[cfg(not(debug_assertions))]
async fn run_watchers() {
    let battery_state = Arc::new(Mutex::new(State::new(StateConfigs::default())));
    let (status_rx, status_watch) = make_status_watcher(battery_state.clone());
    let percent_watch = make_percent_watcher(battery_state, status_rx);
    let (_, _) = tokio::join!(percent_watch, status_watch);
}

#[cfg(debug_assertions)]
async fn run_watchers() {
    let battery_state = Arc::new(Mutex::new(State::new(StateConfigs { min: 60 })));

    let (status_rx, status_watch) = make_status_watcher(battery_state.clone());
    let percent_watch = make_percent_watcher(battery_state, status_rx);

    // let (_, _, _) = tokio::join!(percent_watch, status_watch, battery_state.log_status());
    let (_, _) = tokio::join!(percent_watch, status_watch);
}

mod helper {
    use super::*;

    pub fn file_watcher() -> notify::Result<(PollWatcher, mpsc::Receiver<notify::Result<Event>>)> {
        let (tx, rx) = mpsc::channel(10);

        let watcher = PollWatcher::new(
            move |res| {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    tx.send(res).await.unwrap();
                });
            },
            Config::default()
                .with_compare_contents(true)
                .with_poll_interval(Duration::from_millis(2000)),
        )?;

        Ok((watcher, rx))
    }

    pub fn prog_name() -> Option<String> {
        Some(
            std::env::current_exe()
                .ok()?
                .file_name()?
                .to_string_lossy()
                .to_string(),
        )
    }

    #[allow(dead_code)]
    pub async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
        let (mut watcher, mut rx) = file_watcher()?;

        watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)?;

        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => println!("changed: {:?}", event),
                Err(e) => println!("watch error: {:?}", e),
            }
        }

        Ok(())
    }

    pub fn setup_logging() {
        use env_logger::fmt::Color;
        use std::io::Write;

        #[cfg(not(debug_assertions))]
        env_logger::Builder::new()
            .format(|buf, record| {
                let mut error_style = buf.style();
                error_style.set_color(Color::Red).set_bold(true);

                let mut warn_style = buf.style();
                warn_style.set_color(Color::Rgb(255, 140, 0)).set_bold(true);

                let mut info_style = buf.style();
                info_style.set_color(Color::Green).set_bold(true);

                let mut debug_style = buf.style();
                debug_style.set_color(Color::Yellow).set_bold(true);

                let mut trace_style = buf.style();
                trace_style.set_color(Color::White).set_bold(true);

                let level = record.level();

                let styled_level = match level {
                    log::Level::Error => error_style.value(level),
                    log::Level::Warn => warn_style.value(level),
                    log::Level::Info => info_style.value(level),
                    log::Level::Debug => debug_style.value(level),
                    log::Level::Trace => trace_style.value(level),
                };

                writeln!(
                    buf,
                    "[{} {}] (battery-notify): {}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    styled_level,
                    record.args(),
                )
            })
            .filter_level(log::LevelFilter::Warn)
            .parse_env("LOG_LEVEL")
            .init();

        #[cfg(debug_assertions)]
        env_logger::Builder::new()
            .format(|buf, record| {
                let mut error_style = buf.style();
                error_style.set_color(Color::Red).set_bold(true);

                let mut warn_style = buf.style();
                warn_style.set_color(Color::Rgb(255, 140, 0)).set_bold(true);

                let mut info_style = buf.style();
                info_style.set_color(Color::Green).set_bold(true);

                let mut debug_style = buf.style();
                debug_style.set_color(Color::Yellow).set_bold(true);

                let mut trace_style = buf.style();
                trace_style.set_color(Color::White).set_bold(true);

                let level = record.level();

                let styled_level = match level {
                    log::Level::Error => error_style.value(level),
                    log::Level::Warn => warn_style.value(level),
                    log::Level::Info => info_style.value(level),
                    log::Level::Debug => debug_style.value(level),
                    log::Level::Trace => trace_style.value(level),
                };

                writeln!(
                    buf,
                    "[{} {}] {}::{}:\n- {}\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    styled_level,
                    record.target(),
                    record.line().unwrap_or_default(),
                    record.args(),
                )
            })
            .filter_level(log::LevelFilter::Debug)
            .parse_env("LOG_LEVEL")
            .init();
    }
}
