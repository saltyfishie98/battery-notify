use std::process::Command;

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

pub enum ChargeStatus {
    Discharging { time_remain: chrono::NaiveTime },
    Charging { time_remain: chrono::NaiveTime },
    NotCharging,
}

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

pub fn parse(res: Option<String>) -> Result<Data, ParseError> {
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
        Some(o) => o,
        None => return Err(ParseError::ParseChargeStatus),
    };

    let percent = match data[1].replace("%", "").parse::<u32>() {
        Ok(o) => o,
        Err(_) => return Err(ParseError::ParsePercent),
    };

    let mut remaining_time = None;

    if status_string != "Not Charging" {
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
        "Not charging" => ChargeStatus::NotCharging,
        _ => return Err(ParseError::ParseTimeRemain),
    };

    Ok(Data {
        output: res_str.to_string(),
        battery_id,
        percent,
        status,
    })
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
