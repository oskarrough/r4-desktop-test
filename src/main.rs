//! With a URL, play its audio via libmpv + yt-dlp and exit when it ends. Without one, open the channel list.
use libmpv2::events::Event;

mod app;
mod cache;
mod channels;
mod demo;
mod library;
mod media;
mod player;
mod style;
mod theme;
mod tracks;
mod window;

fn main() {
    let url = std::env::args().nth(1);
    if matches!(url.as_deref(), Some("--version" | "-V")) {
        // Release builds bake in latest-<sha>; see .github/workflows/release.yml.
        println!(
            "radio4000 {}",
            option_env!("RADIO4000_VERSION").unwrap_or("dev")
        );
        return;
    }
    logging();
    let Some(url) = url else {
        if let Err(e) = app::run() {
            eprintln!("radio4000: {e}");
            std::process::exit(1);
        }
        return;
    };
    let mpv = player::mpv(&[]).expect("failed to create mpv");

    mpv.command("loadfile", &[&url]).expect("loadfile failed");

    // Ctrl-C needs no handling: libmpv leaves SIGINT alone, so the process just dies.
    loop {
        let Some(event) = mpv.wait_event(-1.0) else {
            continue;
        };
        player::log_event(&mpv, &event);
        match event {
            Ok(Event::FileLoaded) => {
                let title: String = mpv.get_property("media-title").unwrap_or_default();
                println!("playing: {title}");
            }
            Ok(Event::EndFile(reason)) => {
                let reason = match reason {
                    0 => "eof",
                    2 => "stop",
                    3 => "quit",
                    4 => "error",
                    5 => continue, // redirect: mpv loads the real file next
                    _ => "unknown",
                };
                println!("ended: {reason}");
                std::process::exit(if reason == "error" { 1 } else { 0 });
            }
            Ok(Event::Shutdown) => break,
            // libmpv2 reports a failed end-file as Err, dropping the event itself.
            Err(e) => {
                println!("ended: error ({e})");
                std::process::exit(1);
            }
            _ => {}
        }
    }
}

/// Everything the app keeps on disk lives here: `~/.config/radio4000/desktop/` on Linux. `None` without a home folder.
pub fn app_dir() -> Option<std::path::PathBuf> {
    let dirs = fastframe_dirs::Dirs::platform("org", "radio4000", "radio4000")?;
    Some(dirs.config.join("desktop"))
}

/// `RADIO4000_INSTANCE=<name>` runs a copy beside the usual one, with its own single-instance slot and MPRIS name, so
/// test runs from several checkouts don't hand their launch to each other. `scripts/try` sets it, and its own XDG folders.
pub fn instance() -> Option<String> {
    let name = std::env::var("RADIO4000_INSTANCE").ok()?;
    let name: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Logs to stderr and, unless it's a demo, to `radio4000.log` in [`app_dir`], made fresh each run.
fn logging() {
    let version = option_env!("RADIO4000_VERSION").unwrap_or("dev");
    let mut logging =
        fastframe_log::Logging::new("radio4000", version).filter("warn,radio4000=info");
    if let Some(dir) = app_dir().filter(|_| demo::from_env().is_none()) {
        logging = logging
            .file(dir.join("radio4000.log"))
            .panic_log(dir.join("panic.log"));
    }
    let _ = logging.init();
}
