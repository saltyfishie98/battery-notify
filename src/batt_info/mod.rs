mod battery_data;
mod charge_status;

use std::io::Write;

pub use battery_data::BatteryData;
pub use charge_status::ChargeStatus;

mod helper {
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

pub fn cached() -> Option<BatteryData> {
    let path_str = helper::cache_path();
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
