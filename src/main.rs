mod app;

fn main() {
    app::setup_logging();
    log::info!("Started!");

    let notification = app::BatteryNotification::new(app::Config {
        battery_id: 0,
        battery_low_percent: 20,
    });

    notification.check();
}
