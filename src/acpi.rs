use std::io::Write;
use std::process::Command;

const OLD_ACPI_RES: &str =
    "/home/saltyfishie/.config/waybar/scripts/battery-notify/temp/old_acpi_res.txt";

#[derive(Debug)]
pub enum ParseError {
    ParseTimeRemain,
    ParseInput,
    ParseBatteryId,
    ParseChargeStatus,
    ParsePercent,
    TimeRemainFormat,
    BatteryIdNotI32,
}

#[derive(Debug, PartialEq)]
pub enum ChargeStatus {
    Discharging { time_remain: chrono::NaiveTime },
    Charging { time_remain: chrono::NaiveTime },
    NotCharging,
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
            ChargeStatus::Discharging { time_remain } => write!(
                formatter,
                "{}",
                front(format!("Discharging ({})", time_remain).as_str())
            ),
            ChargeStatus::Charging { time_remain } => {
                write!(
                    formatter,
                    "{}",
                    front(format!("Charging ({})", time_remain).as_str())
                )
            }
            ChargeStatus::NotCharging => write!(formatter, "{}", front("Not Charging")),
        }
    }
}

pub fn parse(res: &Option<String>) -> Result<Data, ParseError> {
    let res_str = match res {
        Some(out) => out,
        None => return Err(ParseError::ParseInput),
    };

    let data = res_str.split(',').map(|s| s.trim()).collect::<Vec<&str>>();

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
            time_remain: remaining_time.unwrap(),
        },
        "Discharging" => ChargeStatus::Discharging {
            time_remain: remaining_time.unwrap(),
        },
        NOT_CHARGING => ChargeStatus::NotCharging,
        _ => return Err(ParseError::ParseTimeRemain),
    };

    Ok(Data {
        output: res_str.to_string(),
        battery_id,
        percent,
        status,
    })
}

pub fn log_to_file(opt_data: &Option<String>) -> std::io::Result<()> {
    let create_file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(OLD_ACPI_RES);

    let mut file = match create_file {
        Ok(f) => f,
        Err(_) => std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(OLD_ACPI_RES)?,
    };

    let data = opt_data.as_ref().ok_or(std::io::ErrorKind::InvalidData)?;

    file.write_all(data.as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn from_file() -> Option<String> {
    Some(std::fs::read_to_string(OLD_ACPI_RES).ok()?)
}

pub fn call() -> Option<String> {
    let output = Command::new("acpi")
        .arg("-b")
        .output()
        .expect("Failed to execute command");

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
    fn parse_charging() {
        let charging = parse(&Some(CHARGHING_STR.to_string())).unwrap();

        assert_eq!(
            charging,
            Data {
                output: CHARGHING_STR.to_string(),
                battery_id: BATTERY_ID,
                percent: 99,
                status: ChargeStatus::Charging {
                    time_remain: chrono::NaiveTime::parse_from_str("00:01:15", "%H:%M:%S").unwrap()
                }
            }
        )
    }

    #[test]
    fn parse_discharging() {
        let discharging = parse(&Some(DISCHARGING_STR.to_string())).unwrap();
        assert_eq!(
            discharging,
            Data {
                output: DISCHARGING_STR.to_string(),
                battery_id: BATTERY_ID,
                percent: 97,
                status: ChargeStatus::Discharging {
                    time_remain: chrono::NaiveTime::parse_from_str("02:20:01", "%H:%M:%S").unwrap()
                }
            }
        )
    }

    #[test]
    fn parse_not_charging() {
        let not_charging = parse(&Some(NOT_CHARGING_STR.to_string())).unwrap();
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
