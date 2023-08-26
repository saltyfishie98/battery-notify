mod acpi {
    use std::process::Command;

    pub enum ChargeStatus {
        Discharging { time_remain: chrono::NaiveTime },
        Charging { time_remain: chrono::NaiveTime },
        NotCharging,
        Unknown,
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
                ChargeStatus::Unknown => write!(f, "Battery Info Unknown"),
            }
        }
    }

    pub fn call() -> Option<Data> {
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

            let battery_id = id_status.next()?.parse::<i32>().ok()?;
            let status_string = id_status.next()?;
            let percent = data[1].replace("%", "").parse::<u32>().ok()?;
            let mut remaining_time = None;

            if status_string != "Not Charging" {
                let remaining_time_str = Some(data[2].split(' ').collect::<Vec<&str>>()[0]);

                remaining_time =
                    Some(chrono::NaiveTime::parse_from_str(remaining_time_str?, "%H:%M:%S").ok()?);
            }

            let status = match status_string.trim() {
                "Charging" => ChargeStatus::Charging {
                    time_remain: remaining_time?,
                },
                "Discharging" => ChargeStatus::Discharging {
                    time_remain: remaining_time?,
                },
                "Not charging" => ChargeStatus::NotCharging,
                _ => ChargeStatus::Unknown,
            };

            Some(Data {
                output: out.to_string(),
                battery_id,
                percent,
                status,
            })
        } else {
            None
        }
    }
}

fn main() {
    match acpi::call() {
        Some(e) => println!("acpi: {}", e),
        None => return,
    };
}
