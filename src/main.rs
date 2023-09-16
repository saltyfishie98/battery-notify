use notify::{Config, Event, PollWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::{path::Path, time::Duration};
use tokio::sync::mpsc::{channel, Receiver};

const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";

#[derive(Debug)]
enum ChargeStatus {
    Charging,
    Discharging,
    NotCharging,
    Unknown,
}

impl From<&str> for ChargeStatus {
    fn from(value: &str) -> Self {
        match value {
            "Charging" => Self::Charging,
            "Not charging" => Self::NotCharging,
            "Discharging" => Self::Discharging,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug)]
struct Battery {
    pub id: u32,
    pub percent: u32,
    pub status: ChargeStatus,
}

#[derive(Debug)]
enum BatteryError {
    BatteryDoesNotExist,
}

impl Battery {
    fn get_live_percent(id: u32) -> Result<u32, BatteryError> {
        let percent_file_string = format!("{POWER_SUPPLY_PATH}/BAT{id}/capacity");
        let percent_file = Path::new(&percent_file_string);

        let string_from_file = match std::fs::read_to_string(percent_file) {
            Ok(out) => out,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(BatteryError::BatteryDoesNotExist);
                } else {
                    log::error!("{:?}", e);
                    panic!();
                }
            }
        };

        match string_from_file.replace("\n", "").parse::<u32>() {
            Ok(out) => return Ok(out),
            Err(err) => {
                log::error!("error: {err}");
                panic!();
            }
        };
    }

    fn get_live_status(id: u32) -> Result<ChargeStatus, BatteryError> {
        let status_file_string = format!("{POWER_SUPPLY_PATH}/BAT{id}/status");
        let status_file = Path::new(&status_file_string);

        let string_from_file = match std::fs::read_to_string(status_file) {
            Ok(out) => out,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(BatteryError::BatteryDoesNotExist);
                } else {
                    log::error!("{:?}", e);
                    panic!();
                }
            }
        };

        Ok(string_from_file.replace("\n", "").as_str().into())
    }
}

struct Batteries {
    pub entry: Vec<Battery>,
}

impl Default for Batteries {
    fn default() -> Self {
        let dir_entries = match std::fs::read_dir(Path::new(POWER_SUPPLY_PATH)) {
            Ok(out) => out,
            Err(e) => {
                log::error!("{}", e);
                panic!()
            }
        };

        let power_supplies: Vec<String> = dir_entries
            .map(|entry| {
                let path = match entry {
                    Ok(ent) => ent.path(),
                    Err(err) => {
                        log::error!("{}", err);
                        panic!();
                    }
                };

                match path.file_name() {
                    Some(out) => out.to_string_lossy().to_string(),
                    None => {
                        log::error!("Sys power supply directory is empty!");
                        panic!();
                    }
                }
            })
            .collect();

        let batt_dirs: Vec<String> = power_supplies
            .into_iter()
            .map(|path_string| {
                if path_string.contains("BAT") {
                    path_string
                } else {
                    "".to_string()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();

        log::debug!("battery directories: {:?}", batt_dirs);

        let mut entries: Vec<Battery> = batt_dirs
            .into_iter()
            .map(|path_string| {
                let re = Regex::new(r"\d+").unwrap();
                let id: u32 = re
                    .find_iter(&path_string)
                    .into_iter()
                    .map(|id_string| id_string.as_str().parse::<u32>().unwrap())
                    .next()
                    .unwrap();

                let percent = Battery::get_live_percent(id).unwrap();
                let status = Battery::get_live_status(id).unwrap();

                Battery {
                    id,
                    percent,
                    status,
                }
            })
            .collect();

        entries.sort_by(|a, b| {
            if a.id < b.id {
                std::cmp::Ordering::Less
            } else if a.id > b.id {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        log::debug!("batteries data: {:?}", entries);

        Self { entry: entries }
    }
}

/// Async, futures channel based event watching
#[tokio::main]
async fn main() {
    setup_logging();

    let _a = &Batteries::default().entry[0];

    let (a, b) = tokio::join!(watch_percent(), watch_status());
    a.unwrap();
    b.unwrap();
}

async fn watch_percent() -> notify::Result<()> {
    let (mut watcher, mut rx) = watcher()?;

    let battery_percent_file = format!("{POWER_SUPPLY_PATH}/BAT0/capacity");
    let percent_path = Path::new(battery_percent_file.as_str());

    watcher.watch(percent_path.as_ref(), RecursiveMode::NonRecursive)?;

    while let Some(res) = rx.recv().await {
        match res {
            Ok(_) => {
                let percent = Battery::get_live_percent(0).unwrap();
                log::info!("battery percent update: {percent}%");
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}

async fn watch_status() -> notify::Result<()> {
    let (mut watcher, mut rx) = watcher()?;

    let battery_status_file = format!("{POWER_SUPPLY_PATH}/BAT0/status");
    let status_path = Path::new(battery_status_file.as_str());

    watcher.watch(status_path.as_ref(), RecursiveMode::NonRecursive)?;

    while let Some(res) = rx.recv().await {
        match res {
            Ok(_) => {
                let status = Battery::get_live_status(0).unwrap();
                log::info!("battery status update: {:?}", status)
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}

fn watcher() -> notify::Result<(PollWatcher, Receiver<notify::Result<Event>>)> {
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
async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
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
