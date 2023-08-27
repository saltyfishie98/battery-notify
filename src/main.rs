use std::process::Command;

mod acpi;

fn main() {
    env_logger::init();

    let res = acpi::call();

    let old_res = acpi::from_file();

    let old = match acpi::parse(&old_res) {
        Ok(out) => out,
        Err(_) => {
            log::error!("Error parsing previous acpi data");
            return;
        }
    };

    let current = match acpi::parse(&res) {
        Ok(out) => out,
        Err(_) => {
            log::error!("Error parsing current acpi data");
            return;
        }
    };

    if current.percent != old.percent
        && current.percent <= 20
        && current.status == acpi::ChargeStatus::Discharging
    {
        match acpi::log_to_file(&res) {
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
        match acpi::log_to_file(&res) {
            Ok(_) => (),
            Err(_) => log::error!("acpi.rs resource error!"),
        };

        let mut charging_command = Command::new("notify-send");

        if current.status == acpi::ChargeStatus::Charging {
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

        if current.status == acpi::ChargeStatus::Discharging
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
