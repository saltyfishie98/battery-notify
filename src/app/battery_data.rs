use super::ChargeStatus;

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BatteryData {
    pub battery_id: i32,
    pub percent: u32,
    pub status: ChargeStatus,
}

impl Default for BatteryData {
    fn default() -> Self {
        Self {
            battery_id: Default::default(),
            percent: Default::default(),
            status: Default::default(),
        }
    }
}

impl std::fmt::Display for BatteryData {
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
            ChargeStatus::Unknown => write!(formatter, "{}", front("Unknown")),
        }
    }
}
