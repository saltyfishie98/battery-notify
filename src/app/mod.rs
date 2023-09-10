use std::{io::Write, process::Command};

mod battery_data;
mod charge_status;

use battery_data::BatteryData;
use charge_status::ChargeStatus;

pub fn setup_logging() {
    use env_logger::fmt::Color;

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

pub struct BatteryNotification {
    current_json: String,
    current: BatteryData,
    cached: BatteryData,
    battery_low_percent: u32,
    battery_id: usize,
}

pub struct Config {
    pub battery_id: u32,
    pub battery_low_percent: u32,
}

impl BatteryNotification {
    pub fn new(config: Config) -> Self {
        let battery_id = config.battery_id as usize;

        let current = match helper::get(battery_id) {
            Some(out) => out,
            None => panic!(),
        };

        let current_json = serde_json::to_string_pretty(&current).unwrap();

        let cached = match helper::cached(battery_id) {
            Some(out) => out,
            None => {
                match helper::log_to_file(&current_json, battery_id) {
                    Ok(_) => (),
                    Err(e) => log::error!("{}", e),
                };

                match serde_json::from_str(&current_json) {
                    Ok(out) => out,
                    Err(e) => {
                        log::error!("{}", e);
                        panic!();
                    }
                }
            }
        };

        Self {
            current_json,
            current,
            cached,
            battery_low_percent: config.battery_low_percent,
            battery_id,
        }
    }

    pub fn check(&self) {
        if self.current.percent != self.cached.percent
            && self.current.percent <= self.battery_low_percent
            && self.current.status == (ChargeStatus::Discharging { time_remain: None })
        {
            match helper::log_to_file(&self.current_json, self.battery_id) {
                Ok(_) => (),
                Err(_) => log::error!("acpi.rs resource error!"),
            };

            let mut batt_low_command = Command::new("notify-send");

            let batt_low_notify = batt_low_command.args([
                "-u",
                "critical",
                "-t",
                "5000",
                "Battery Low",
                format!("Battery charge at {}%!", self.current.percent).as_str(),
            ]);

            match batt_low_notify.status() {
                Ok(res) => {
                    if !res.success() {
                        log::error!("Command failed with exit code: {:?}", res.code());
                    }
                }
                Err(err) => log::error!("Error executing command: {}", err),
            }
        }

        if self.current.status != self.cached.status {
            match helper::log_to_file(&self.current_json, self.battery_id) {
                Ok(_) => (),
                Err(_) => log::error!("acpi.rs resource error!"),
            };

            let mut charging_command = Command::new("notify-send");

            if self.current.status == (ChargeStatus::Charging { time_remain: None }) {
                let charging_notify = charging_command.args([
                    "-t",
                    "2000",
                    "Battery Status Update",
                    "The battery has started charging!",
                ]);

                match charging_notify.status() {
                    Ok(res) => {
                        if !res.success() {
                            log::error!("Command failed with exit code: {:?}", res.code());
                        }
                    }
                    Err(err) => log::error!("Error executing command: {}", err),
                }
            }

            if self.current.status == (ChargeStatus::Discharging { time_remain: None })
                || self.current.status == ChargeStatus::NotCharging
            {
                let charging_notify = charging_command.args([
                    "-t",
                    "1500",
                    "Battery Status Update",
                    "The battery has stopped charging!",
                ]);

                match charging_notify.status() {
                    Ok(res) => {
                        if !res.success() {
                            log::error!("Command failed with exit code: {:?}", res.code());
                        }
                    }
                    Err(err) => log::error!("Error executing command: {}", err),
                }
            }
        }
    }
}

mod helper {
    use super::*;

    pub fn cache_path(id: usize) -> String {
        #[cfg(debug_assertions)]
        let path = format!("./cache/battery_{}.json", id);

        #[cfg(not(debug_assertions))]
        let path = format!(
            "{}/.cache/battery-notify/battery_{}.json",
            std::env::var("HOME").unwrap(),
            id
        );

        log::debug!("cache path: {}", path);
        path
    }

    pub fn log_to_file(data: &str, id: usize) -> std::io::Result<()> {
        let path_str = helper::cache_path(id);
        let path = std::path::Path::new(path_str.as_str());

        if !path.exists() {
            let prefix = path.parent().unwrap();
            log::debug!("{}", prefix.to_str().unwrap());
            std::fs::create_dir_all(prefix)?;
        }

        let create_file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path);

        let mut file = match create_file {
            Ok(f) => f,
            Err(_) => std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)?,
        };

        file.write_all(data.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    pub fn cached(id: usize) -> Option<BatteryData> {
        let path_str = helper::cache_path(id);
        let path = std::path::Path::new(path_str.as_str());

        match std::fs::metadata(path) {
            Ok(_) => (),
            Err(e) => log::warn!("{}", e),
        }

        let res = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&res).ok()?
    }

    pub fn get(id: usize) -> Option<BatteryData> {
        let manager = battery::Manager::new().ok()?;

        let batteries = manager
            .batteries()
            .ok()?
            .map(|i| i.ok())
            .collect::<Vec<Option<battery::Battery>>>();

        if let Some(battery) = &batteries[id] {
            let out = BatteryData {
                battery_id: id as i32,
                percent: (f32::from(battery.energy() / battery.energy_full()) * 100.0) as u32,
                status: match battery.state() {
                    battery::State::Unknown => ChargeStatus::Unknown,
                    battery::State::Charging => ChargeStatus::Charging {
                        time_remain: Some(
                            chrono::NaiveTime::default()
                                + chrono::Duration::milliseconds(
                                    (battery.time_to_full()?.value * 1000.0) as i64,
                                ),
                        ),
                    },
                    battery::State::Discharging => ChargeStatus::Discharging {
                        time_remain: Some(
                            chrono::NaiveTime::default()
                                + chrono::Duration::milliseconds(
                                    (battery.time_to_empty()?.value * 1000.0) as i64,
                                ),
                        ),
                    },
                    battery::State::Empty => ChargeStatus::NotCharging,
                    battery::State::Full => ChargeStatus::NotCharging,
                    _ => panic!(),
                },
            };

            log::debug!("{:?}", out);

            Some(out)
        } else {
            None
        }
    }
}
