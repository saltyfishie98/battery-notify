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
    #[cfg(not(debug_assertions))]
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {}] ({}): {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                prog_name().unwrap_or("saltyfishie".to_string()),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stdout())
        .apply()
        .unwrap();

    #[cfg(debug_assertions)]
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}::{}\n- {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                record.line().unwrap_or_default(),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .level_for("notify", log::LevelFilter::Info)
        .level_for("mio", log::LevelFilter::Info)
        .chain(std::io::stdout())
        .apply()
        .unwrap();
}
