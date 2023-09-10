mod charge_status;
mod data;

use std::io::Write;
use std::process::Command;

pub use charge_status::ChargeStatus;
pub use data::Data;

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
    use super::*;

    #[cfg(not(debug_assertions))]
    pub fn cache_path() -> String {
        let path = format!(
            "{}/.cache/battery-notify/cache.json",
            std::env::var("HOME").unwrap()
        );
        log::debug!("cache path: {}", path);
        path
    }

    #[cfg(debug_assertions)]
    pub fn cache_path() -> String {
        let path = "./cache/data.json".to_string();
        log::debug!("cache path: {}", path);
        path
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

    #[cfg(test)]
    mod test {
        use super::*;

        const BATTERY_ID: i32 = 0;
        const CHARGHING_STR: &str = "Battery 0: Charging, 99%, 00:01:15 until charged";
        const DISCHARGING_STR: &str = "Battery 0: Discharging, 97%, 02:20:01 remaining";
        const NOT_CHARGING_STR: &str = "Battery 0: Not charging, 100%";

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

pub fn from_file() -> Option<Data> {
    let path_str = helper::cache_path();
    let path = std::path::Path::new(path_str.as_str());

    match std::fs::metadata(path) {
        Ok(_) => (),
        Err(e) => log::warn!("{}", e),
    }

    let res = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&res).ok()?)
}

pub fn call() -> Option<Data> {
    let output_res = Command::new("acpi").arg("-b").output();

    let output = match output_res {
        Ok(out) => out,
        Err(_) => {
            log::error!("\"acpi\" command not found! (is acpi installed?)");
            panic!();
        }
    };

    let out = String::from_utf8_lossy(&output.stdout).to_string();
    log::debug!("{}", out);

    if output.status.success() {
        Some(helper::parse(&out).ok()?)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_status_partial_eq_same_time() {
        let a = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        let b = ChargeStatus::Charging {
            time_remain: Some(chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()),
        };

        let json = serde_json::to_string_pretty(&a).unwrap();
        println!("{}", json);

        let status: ChargeStatus = serde_json::from_str(json.as_str()).unwrap();
        println!("{:?}", status);

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
}
