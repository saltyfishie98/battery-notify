use std::process::Command;

mod acpi;

fn main() {
    env_logger::init();

    let current = match acpi::call() {
        Some(out) => out,
        None => todo!(),
    };

    let json_str = serde_json::to_string_pretty(&current).unwrap();

    let old = match acpi::from_file() {
        Some(out) => out,
        None => {
            match acpi::log_to_file(&json_str) {
                Ok(_) => (),
                Err(e) => log::error!("{}", e),
            };

            match acpi::call() {
                Some(out) => out,
                None => todo!(),
            }
        }
    };

    if current.percent != old.percent
        && current.percent <= 20
        && current.status == (acpi::ChargeStatus::Discharging { time_remain: None })
    {
        match acpi::log_to_file(&json_str) {
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
        match acpi::log_to_file(&json_str) {
            Ok(_) => (),
            Err(_) => log::error!("acpi.rs resource error!"),
        };

        let mut charging_command = Command::new("notify-send");

        if current.status == (acpi::ChargeStatus::Charging { time_remain: None }) {
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

        if current.status == (acpi::ChargeStatus::Discharging { time_remain: None })
            || current.status == acpi::ChargeStatus::NotCharging
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
