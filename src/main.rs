mod acpi;

fn main() {
    let res = acpi::call();

    let old_res = acpi::from_file();

    let old = match acpi::parse(&old_res) {
        Ok(out) => out,
        Err(_) => {
            eprintln!("Error parsing acpi data");
            return;
        }
    };

    let current = match acpi::parse(&res) {
        Ok(out) => out,
        Err(_) => {
            eprintln!("Error parsing acpi data");
            return;
        }
    };

    if current.percent != old.percent {
        println!("old: {}", old);
        println!("\nnew: {}", current);

        match acpi::log_to_file(&res) {
            Ok(_) => (),
            Err(e) => eprintln!("Error logging to file: {}", e),
        }
    }
}
