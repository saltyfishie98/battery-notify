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

    if current.percent != old.percent && current.percent <= 20 {
        match acpi::log_to_file(&res) {
            Ok(_) => (),
            Err(_) => log::error!("acpi.rs resource failure!"),
        };

        let mut command = Command::new("notify-send");

        let notify = command.args([
            "Battery Low",
            format!("Battery charge at {}%!", current.percent).as_str(),
        ]);

        match notify.status() {
            Ok(res) => {
                if !res.success() {
                    log::error!("Command failed with exit code: {:?}", res.code());
                }
            }
            Err(err) => log::error!("Error executing command: {}", err),
        }
    }
}
