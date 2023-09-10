use std::process::Command;

mod batt_info;

fn main() {
    #[cfg(not(debug_assertions))]
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {}] (battery-notify) - {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args(),
            )
        })
        .filter(None, log::LevelFilter::Trace)
        .init();

    #[cfg(debug_assertions)]
    env_logger::init();

    let current = match batt_info::get(0) {
        Some(out) => out,
        None => panic!(),
    };

    let json_str = serde_json::to_string_pretty(&current).unwrap();

    let old = match batt_info::cached() {
        Some(out) => out,
        None => {
            match batt_info::log_to_file(&json_str) {
                Ok(_) => (),
                Err(e) => log::error!("{}", e),
            };

            match batt_info::get(0) {
                Some(out) => out,
                None => todo!(),
            }
        }
    };

    if current.percent != old.percent
        && current.percent <= 20
        && current.status == (batt_info::ChargeStatus::Discharging { time_remain: None })
    {
        match batt_info::log_to_file(&json_str) {
            Ok(_) => (),
            Err(_) => log::error!("acpi.rs resource error!"),
        };

        let mut batt_low_command = Command::new("notify-send");

        let batt_low_notify = batt_low_command.args([
            "-u",
            "critical",
            "Battery Low",
            format!("Battery charge at {}%!", current.percent).as_str(),
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

    if current.status != old.status {
        match batt_info::log_to_file(&json_str) {
            Ok(_) => (),
            Err(_) => log::error!("acpi.rs resource error!"),
        };

        let mut charging_command = Command::new("notify-send");

        if current.status == (batt_info::ChargeStatus::Charging { time_remain: None }) {
            let charging_notify = charging_command
                .args(["Battery Status Update", "The battery has started charging!"]);

            match charging_notify.status() {
                Ok(res) => {
                    if !res.success() {
                        log::error!("Command failed with exit code: {:?}", res.code());
                    }
                }
                Err(err) => log::error!("Error executing command: {}", err),
            }
        }

        if current.status == (batt_info::ChargeStatus::Discharging { time_remain: None })
            || current.status == batt_info::ChargeStatus::NotCharging
        {
            let charging_notify = charging_command
                .args(["Battery Status Update", "The battery has stopped charging!"]);

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
