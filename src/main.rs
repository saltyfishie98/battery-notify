use notify::{Config, Event, PollWatcher, RecursiveMode, Watcher};
use notify_rust::{Hint, Notification};
use std::{
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::sync::watch;

mod battery;

struct State {
    battery_state: Arc<Mutex<battery::Battery>>,
    min_battery_percent: u32,
}

struct StateConfigs {
    min: u32,
}

impl Default for StateConfigs {
    fn default() -> Self {
        Self { min: 20 }
    }
}

impl State {
    fn new(config: StateConfigs) -> Self {
        State {
            battery_state: Arc::new(Mutex::new(battery::Batteries::default().entry.remove(0))),
            min_battery_percent: config.min,
        }
    }

    #[cfg(debug_assertions)]
    fn log_status(&self) -> impl Future<Output = ()> {
        let state_watcher = self.battery_state.clone();
        async move {
            loop {
                let duration = tokio::time::Duration::from_secs(2);
                log::info!("latest battery state: {:?}", state_watcher.lock().unwrap());
                tokio::time::sleep(duration).await;
            }
        }
    }

    fn make_percent_watcher(
        &self,
        status_recv: watch::Receiver<battery::ChargeStatus>,
    ) -> (
        watch::Receiver<u32>,
        impl Future<Output = notify::Result<()>> + '_,
    ) {
        let (_tx, rx) = watch::channel(0);

        (rx, async move {
            let percent_mutex = self.battery_state.clone();
            let battery_percent_file = battery::percent_path(0);
            let percent_path = Path::new(battery_percent_file.as_str());
            let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;

            file_watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

            while let Some(res) = file_watcher_rx.recv().await {
                let mut status_recv_1 = status_recv.clone();

                match res {
                    Ok(_) => {
                        let percent = battery::Battery::get_live_percent(0).unwrap();
                        let mut battery_state = percent_mutex.lock().unwrap();

                        let is_charging = battery_state.status == battery::ChargeStatus::Charging;
                        let low_battery = percent <= self.min_battery_percent;

                        if low_battery && !is_charging {
                            // TODO: only one instance of battery low notification!
                            log::debug!("new battery notification @ {percent}%");
                            tokio::spawn(async move {
                                match Notification::new()
                                    .summary("battery-notify")
                                    .body("battery charge is low!")
                                    .hint(Hint::Transient(true))
                                    .urgency(notify_rust::Urgency::Critical)
                                    .timeout(Duration::from_millis(0))
                                    .show()
                                {
                                    Ok(handle) => {
                                        if let Err(e) = status_recv_1
                                            .wait_for(|status| {
                                                *status == battery::ChargeStatus::Charging
                                            })
                                            .await
                                        {
                                            log::error!("status receiver error: {:?}", e);
                                        }
                                        handle.close();
                                        log::debug!("battery notification close!");
                                    }
                                    Err(e) => {
                                        log::error!("percent notification error: {:?}", e);
                                    }
                                }
                            });
                        }

                        battery_state.percent = percent;
                    }
                    Err(e) => println!("watch error: {:?}", e),
                }
            }

            Ok(())
        })
    }

    fn make_status_watcher(
        &self,
    ) -> (
        watch::Receiver<battery::ChargeStatus>,
        impl Future<Output = notify::Result<()>> + '_,
    ) {
        let (tx, rx) = watch::channel(battery::ChargeStatus::Unknown);

        (rx, async move {
            let status_mutex = self.battery_state.clone();
            let (mut file_watcher, mut file_watcher_rx) = helper::file_watcher()?;

            let battery_status_file = battery::status_path(0);
            let status_path = Path::new(battery_status_file.as_str());

            file_watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

            while let Some(res) = file_watcher_rx.recv().await {
                match res {
                    Ok(_) => {
                        let new_status = battery::Battery::get_live_status(0).unwrap();
                        let mut battery_state = status_mutex.lock().unwrap();

                        if new_status != battery_state.status {
                            match new_status {
                                battery::ChargeStatus::Charging => {
                                    if let Err(e) = tx.send(battery::ChargeStatus::Charging) {
                                        log::error!("status sender error: {}", e);
                                    }

                                    if let Err(e) = Notification::new()
                                        .summary("battery-notify")
                                        .body("The battery has started charging!")
                                        .hint(Hint::Transient(true))
                                        .show()
                                    {
                                        log::error!("status notification error: {:?}", e);
                                    }
                                }

                                battery::ChargeStatus::Discharging
                                | battery::ChargeStatus::NotCharging => {
                                    if let Err(e) = tx.send(battery::ChargeStatus::NotCharging) {
                                        log::error!("status watch channel error: {}", e);
                                    }

                                    if let Err(e) = Notification::new()
                                        .summary("battery-notify")
                                        .body("The battery has stopped charging!")
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
                                        .summary("battery-notify")
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
}

#[tokio::main]
async fn main() {
    helper::setup_logging();
    run_watchers().await;
}

#[cfg(not(debug_assertions))]
async fn run_watchers() {
    let battery_state = State::new(StateConfigs::default());

    let (status_rx, status_watch) = battery_state.make_status_watcher();
    let (_, percent_watch) = battery_state.make_percent_watcher(status_rx);

    let (a, b) = tokio::join!(percent_watch, status_watch);

    a.unwrap();
    b.unwrap();
}

#[cfg(debug_assertions)]
async fn run_watchers() {
    let battery_state = State::new(StateConfigs { min: 60 });

    let (status_rx, status_watch) = battery_state.make_status_watcher();
    let (_, percent_watch) = battery_state.make_percent_watcher(status_rx);

    // let (a, b, _) = tokio::join!(percent_watch, status_watch, battery_state.log_status());
    let (a, b) = tokio::join!(percent_watch, status_watch);

    a.unwrap();
    b.unwrap();
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
