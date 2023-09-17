use notify::{Config, Event, PollWatcher, RecursiveMode, Watcher};
use std::{
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc::{channel, Receiver};

mod battery;

struct State {
    battery_state: Arc<Mutex<battery::Battery>>,
}

impl State {
    fn new() -> Self {
        State {
            battery_state: Arc::new(Mutex::new(battery::Batteries::default().entry.remove(0))),
        }
    }

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

    fn make_percent_watcher(&self) -> impl Future<Output = notify::Result<()>> {
        let percent_mutex = self.battery_state.clone();
        async move {
            let (mut watcher, mut rx) = helper::watcher()?;

            let battery_percent_file = battery::percent_path(0);
            let percent_path = Path::new(battery_percent_file.as_str());

            watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

            while let Some(res) = rx.recv().await {
                match res {
                    Ok(_) => {
                        let percent = battery::Battery::get_live_percent(0).unwrap();
                        let mut battery_state = percent_mutex.lock().unwrap();
                        log::info!("battery percent update: {percent}%");
                        battery_state.percent = percent;
                    }
                    Err(e) => println!("watch error: {:?}", e),
                }
            }

            Ok(())
        }
    }

    fn make_status_watcher(&self) -> impl Future<Output = notify::Result<()>> {
        let status_mutex = self.battery_state.clone();
        async move {
            let (mut watcher, mut rx) = helper::watcher()?;

            let battery_status_file = battery::status_path(0);
            let status_path = Path::new(battery_status_file.as_str());

            watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

            while let Some(res) = rx.recv().await {
                match res {
                    Ok(_) => {
                        let status = battery::Battery::get_live_status(0).unwrap();
                        let mut battery_state = status_mutex.lock().unwrap();
                        log::info!("battery status update: {:?}", status);
                        battery_state.status = status;
                    }
                    Err(e) => println!("watch error: {:?}", e),
                }
            }

            Ok(())
        }
    }
}

/// Async, futures channel based event watching
#[tokio::main]
async fn main() {
    helper::setup_logging();
    run_watchers().await;
}

#[cfg(not(debug_assertions))]
async fn run_watchers() {
    let battery_state = State::new();
    let (a, b) = tokio::join!(
        battery_state.make_percent_watcher(),
        battery_state.make_status_watcher(),
    );
    a.unwrap();
    b.unwrap();
}

#[cfg(debug_assertions)]
async fn run_watchers() {
    let battery_state = State::new();
    let (a, b, _) = tokio::join!(
        battery_state.make_percent_watcher(),
        battery_state.make_status_watcher(),
        battery_state.log_status(),
    );
    a.unwrap();
    b.unwrap();
}

mod helper {
    use super::*;

    pub fn watcher() -> notify::Result<(PollWatcher, Receiver<notify::Result<Event>>)> {
        let (tx, rx) = channel(10);

        let watcher = PollWatcher::new(
            move |res| {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    tx.send(res).await.unwrap();
                });
            },
            Config::default()
                .with_compare_contents(true)
                .with_poll_interval(Duration::from_millis(1500)),
        )?;

        Ok((watcher, rx))
    }

    #[allow(dead_code)]
    pub async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
        let (mut watcher, mut rx) = watcher()?;

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
            .filter_level(log::LevelFilter::Info)
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
