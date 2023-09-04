use std::io::Write;
use std::process::Command;

#[derive(Debug)]
pub enum ParseError {
    ParseTimeRemain,
    ParseBatteryId,
    ParseChargeStatus,
    ParsePercent,
    TimeRemainFormat,
    BatteryIdNotI32,
}

mod helper {
    pub fn cache_path() -> String {
        let path = format!(
            "{}/.cache/battery-notify/cache.txt",
            std::env::var("HOME").unwrap()
        );
        log::debug!("cache path: {}", path);
        path
    }
}

#[derive(Debug)]
pub enum ChargeStatus {
    Discharging {
        time_remain: Option<chrono::NaiveTime>,
    },
    Charging {
        time_remain: Option<chrono::NaiveTime>,
    },
    NotCharging,
}

impl PartialEq for ChargeStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Discharging { time_remain: _ }, Self::Discharging { time_remain: _ }) => true,
            (Self::Charging { time_remain: _ }, Self::Charging { time_remain: _ }) => true,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Data {
    pub output: String,
    pub battery_id: i32,
    pub percent: u32,
    pub status: ChargeStatus,
}

impl std::fmt::Display for Data {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let front = |status: &str| -> String {
            format!(
                "Battery Info:\n\tid: {}\n\tpercent: {}\n\tstatus: {}",
                self.battery_id, self.percent, status
            )
        };

        match self.status {
            ChargeStatus::Discharging { time_remain } => {
                let time_remain_str: String = match time_remain {
                    Some(out) => format!("{}", out),
                    None => "unknown".to_string(),
                };

                write!(
                    formatter,
                    "{}",
                    front(format!("Discharging ( time remaining: {} )", time_remain_str).as_str())
                )
            }
            ChargeStatus::Charging { time_remain } => {
                let time_remain_str: String = match time_remain {
                    Some(out) => format!("{}", out),
                    None => "unknown".to_string(),
                };

                write!(
                    formatter,
                    "{}",
                    front(format!("Charging ( time remaining: {} )", time_remain_str).as_str())
                )
            }
            ChargeStatus::NotCharging => write!(formatter, "{}", front("Not Charging")),
        }
    }
}

pub fn parse(res: &str) -> Result<Data, ParseError> {
    let data = res.split(',').map(|s| s.trim()).collect::<Vec<&str>>();

    let mut id_status = data[0].split(':').map(|s| {
        if s.contains("Battery ") {
            s.to_string().replace("Battery ", "")
        } else {
            s.to_string()
        }
    });

    let battery_id = match id_status.next() {
        Some(id) => match id.parse::<i32>() {
            Ok(o) => o,
            Err(_) => return Err(ParseError::BatteryIdNotI32),
        },
        None => return Err(ParseError::ParseBatteryId),
    };

    let status_string = match id_status.next() {
        Some(o) => o.trim().to_string(),
        None => return Err(ParseError::ParseChargeStatus),
    };

    let percent = match data[1].replace("%", "").parse::<u32>() {
        Ok(o) => o,
        Err(_) => return Err(ParseError::ParsePercent),
    };

    let mut remaining_time = None;

    const NOT_CHARGING: &str = "Not charging";

    if status_string != NOT_CHARGING {
        let remaining_time_str = Some(data[2].split(' ').collect::<Vec<&str>>()[0]);

        remaining_time = Some(
            match chrono::NaiveTime::parse_from_str(remaining_time_str.unwrap(), "%H:%M:%S") {
                Ok(o) => o,
                Err(_) => return Err(ParseError::TimeRemainFormat),
            },
        );
    }

    let status = match status_string.trim() {
        "Charging" => ChargeStatus::Charging {
            time_remain: remaining_time,
        },
        "Discharging" => ChargeStatus::Discharging {
            time_remain: remaining_time,
        },
        NOT_CHARGING => ChargeStatus::NotCharging,
        _ => return Err(ParseError::ParseTimeRemain),
    };

    Ok(Data {
        output: res.to_string(),
        battery_id,
        percent,
        status,
    })
}

pub fn log_to_file(data: &str) -> std::io::Result<()> {
    let path_str = helper::cache_path();
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

pub fn from_file() -> Option<String> {
    let path_str = helper::cache_path();
    let path = std::path::Path::new(path_str.as_str());

    match std::fs::metadata(path) {
        Ok(_) => (),
        Err(e) => log::warn!("{}", e),
    }
    Some(std::fs::read_to_string(path).ok()?)
}

pub fn call() -> Option<String> {
    let output_res = Command::new("acpi").arg("-b").output();

    let output = match output_res {
        Ok(out) => out,
        Err(_) => {
            log::error!("\"acpi\" command not found! (is acpi installed?)");
            panic!();
        }
    };

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BATTERY_ID: i32 = 0;
    const CHARGHING_STR: &str = "Battery 0: Charging, 99%, 00:01:15 until charged";
    const DISCHARGING_STR: &str = "Battery 0: Discharging, 97%, 02:20:01 remaining";
    const NOT_CHARGING_STR: &str = "Battery 0: Not charging, 100%";

    #[test]
    fn charge_status_partial_eq_same_time() {
        let a = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        let b = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        assert!(a == b);
    }

    #[test]
    fn charge_status_partial_eq_diff_time() {
        let a = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:00:00", "%H:%M:%S").unwrap()),
        };

        let b = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        assert!(a == b);
    }

    #[test]
    fn charge_status_partial_eq_none() {
        let a = ChargeStatus::Charging { time_remain: None };

        let b = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        assert!(a == b);
    }

    #[test]
    fn parse_charging() {
        let charging = parse(CHARGHING_STR).unwrap();

        assert_eq!(
            charging,
            Data {
                output: CHARGHING_STR.to_string(),
                battery_id: BATTERY_ID,
                percent: 99,
                status: ChargeStatus::Charging {
                    time_remain: Some(
                        chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()
                    )
                }
            }
        )
    }

    #[test]
    fn parse_discharging() {
        let discharging = parse(DISCHARGING_STR).unwrap();
        assert_eq!(
            discharging,
            Data {
                output: DISCHARGING_STR.to_string(),
                battery_id: BATTERY_ID,
                percent: 97,
                status: ChargeStatus::Discharging {
                    time_remain: Some(
                        chrono::NaiveTime::parse_from_str("02:20:01", "%H:%M:%S").unwrap()
                    )
                }
            }
        )
    }

    #[test]
    fn parse_not_charging() {
        let not_charging = parse(NOT_CHARGING_STR).unwrap();
        assert_eq!(
            not_charging,
            Data {
                output: NOT_CHARGING_STR.to_string(),
                battery_id: BATTERY_ID,
                percent: 100,
                status: ChargeStatus::NotCharging,
            }
        )
    }
}
