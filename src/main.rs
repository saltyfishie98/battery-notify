mod acpi;

fn main() {
    match acpi::call() {
        Ok(o) => println!("acpi:{}", o),
        Err(_) => println!("Error!"),
    };
}
