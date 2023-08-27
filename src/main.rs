mod acpi;

fn main() {
    let res = acpi::call();
    match acpi::parse(res) {
        Ok(out) => println!("{}", out),
        Err(_) => todo!(),
    }
}
