use notify::{Config, Event, PollWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

pub fn file_watcher() -> notify::Result<(PollWatcher, mpsc::Receiver<notify::Result<Event>>)> {
    let (tx, rx) = mpsc::channel(10);

    let watcher = PollWatcher::new(
        move |res| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                tx.send(res).await.unwrap();
            });
        },
        Config::default()
            .with_compare_contents(true)
            .with_poll_interval(Duration::from_millis(2000)),
    )?;

    Ok((watcher, rx))
}

pub fn prog_name() -> Option<String> {
    Some(
        std::env::current_exe()
            .ok()?
            .file_name()?
            .to_string_lossy()
            .to_string(),
    )
}

#[allow(dead_code)]
pub async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let (mut watcher, mut rx) = file_watcher()?;

    watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)?;

    while let Some(res) = rx.recv().await {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}

pub fn setup_logging() {
    use env_logger::fmt::Color;
    use std::io::Write;

    #[cfg(not(debug_assertions))]
    env_logger::Builder::new()
        .format(|buf, record| {
            let mut error_style = buf.style();
            error_style.set_color(Color::Red).set_bold(true);

            let mut warn_style = buf.style();
            warn_style.set_color(Color::Rgb(255, 140, 0)).set_bold(true);

            let mut info_style = buf.style();
            info_style.set_color(Color::Green).set_bold(true);

            let mut debug_style = buf.style();
            debug_style.set_color(Color::Yellow).set_bold(true);

            let mut trace_style = buf.style();
            trace_style.set_color(Color::White).set_bold(true);

            let level = record.level();

            let styled_level = match level {
                log::Level::Error => error_style.value(level),
                log::Level::Warn => warn_style.value(level),
                log::Level::Info => info_style.value(level),
                log::Level::Debug => debug_style.value(level),
                log::Level::Trace => trace_style.value(level),
            };

            writeln!(
                buf,
                "[{} {}] (battery-notify): {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                styled_level,
                record.args(),
            )
        })
        .filter_level(log::LevelFilter::Warn)
        .parse_env("LOG_LEVEL")
        .init();

    #[cfg(debug_assertions)]
    env_logger::Builder::new()
        .format(|buf, record| {
            let mut error_style = buf.style();
            error_style.set_color(Color::Red).set_bold(true);

            let mut warn_style = buf.style();
            warn_style.set_color(Color::Rgb(255, 140, 0)).set_bold(true);

            let mut info_style = buf.style();
            info_style.set_color(Color::Green).set_bold(true);

            let mut debug_style = buf.style();
            debug_style.set_color(Color::Yellow).set_bold(true);

            let mut trace_style = buf.style();
            trace_style.set_color(Color::White).set_bold(true);

            let level = record.level();

            let styled_level = match level {
                log::Level::Error => error_style.value(level),
                log::Level::Warn => warn_style.value(level),
                log::Level::Info => info_style.value(level),
                log::Level::Debug => debug_style.value(level),
                log::Level::Trace => trace_style.value(level),
            };

            writeln!(
                buf,
                "[{} {}] {}::{}:\n- {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                styled_level,
                record.target(),
                record.line().unwrap_or_default(),
                record.args(),
            )
        })
        .filter_level(log::LevelFilter::Debug)
        .parse_env("LOG_LEVEL")
        .init();
}
