use std::process::Command;

#[derive(Debug)]
pub enum Error {
    TimeRemain,
    ParseTimeRemain,
    Exec,
    BatteryId,
    ParseBatteryId,
    ChargeStatus,
    ParsePercent,
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let front = |status: &str| -> String {
            format!(
                "\n\tBattery Info:\n\t\tid: {}\n\t\tpercent: {}\n\t\tstatus: {}",
                self.battery_id, self.percent, status
            )
        };

        match self.status {
            ChargeStatus::Discharging { time_remain } => write!(
                f,
                "{}",
                front(format!("Discharging ({})", time_remain).as_str())
            ),
            ChargeStatus::Charging { time_remain } => {
                write!(
                    f,
                    "{}",
                    front(format!("Charging ({})", time_remain).as_str())
                )
            }
            ChargeStatus::NotCharging => write!(f, "{}", front("Not Charging")),
        }
    }
}

pub fn call() -> Result<Data, Error> {
    let output = Command::new("acpi")
        .arg("-b")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let out = String::from_utf8_lossy(&output.stdout);
        let data = out.split(',').map(|s| s.trim()).collect::<Vec<&str>>();

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
                Err(_) => return Err(Error::ParseBatteryId),
            },
            None => return Err(Error::BatteryId),
        };

        let status_string = match id_status.next() {
            Some(o) => o,
            None => return Err(Error::ChargeStatus),
        };

        let percent = match data[1].replace("%", "").parse::<u32>() {
            Ok(o) => o,
            Err(_) => return Err(Error::ParsePercent),
        };

        let mut remaining_time = None;

        if status_string != "Not Charging" {
            let remaining_time_str = Some(data[2].split(' ').collect::<Vec<&str>>()[0]);

            remaining_time = Some(
                match chrono::NaiveTime::parse_from_str(remaining_time_str.unwrap(), "%H:%M:%S") {
                    Ok(o) => o,
                    Err(_) => return Err(Error::ParseTimeRemain),
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
            _ => return Err(Error::TimeRemain),
        };

        Ok(Data {
            output: out.to_string(),
            battery_id,
            percent,
            status,
        })
    } else {
        Err(Error::Exec)
    }
}
