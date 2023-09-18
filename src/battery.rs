use regex::Regex;
use std::path::Path;

const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";

pub fn percent_path(id: u32) -> String {
    format!("{POWER_SUPPLY_PATH}/BAT{id}/capacity")
}

pub fn status_path(id: u32) -> String {
    format!("{POWER_SUPPLY_PATH}/BAT{id}/status")
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ChargeStatus {
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
pub struct Battery {
    pub id: u32,
    pub percent: u32,
    pub status: ChargeStatus,
}

#[derive(Debug)]
pub enum BatteryError {
    BatteryDoesNotExist,
}

impl Battery {
    pub fn get_live_percent(id: u32) -> Result<u32, BatteryError> {
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

        match string_from_file.replace('\n', "").parse::<u32>() {
            Ok(out) => Ok(out),
            Err(err) => {
                log::error!("error: {err}");
                panic!();
            }
        }
    }

    pub fn get_live_status(id: u32) -> Result<ChargeStatus, BatteryError> {
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

        Ok(string_from_file.replace('\n', "").as_str().into())
    }
}

pub struct Batteries {
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

        log::info!("battery directories: {:?}", batt_dirs);

        let mut entries: Vec<Battery> = batt_dirs
            .into_iter()
            .map(|path_string| {
                let re = Regex::new(r"\d+").unwrap();
                let id: u32 = re
                    .find_iter(&path_string)
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

        entries.sort_by(|a, b| a.id.cmp(&b.id));

        log::info!("default batteries data: {:?}", entries);

        Self { entry: entries }
    }
}
