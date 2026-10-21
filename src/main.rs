//! With a URL, play its audio via libmpv + yt-dlp and exit when it ends. Without one, open the channel list.
use libmpv2::events::Event;

mod channels;
mod demo;
mod gui;
mod player;
mod style;
mod theme;
mod tracks;

fn main() {
    let Some(url) = std::env::args().nth(1) else {
        if let Err(e) = gui::run() {
            eprintln!("radio4000: {e}");
            std::process::exit(1);
        }
        return;
    };
    if url == "--version" || url == "-V" {
        // Release builds bake in latest-<sha>; see .github/workflows/release.yml.
        println!(
            "radio4000 {}",
            option_env!("RADIO4000_VERSION").unwrap_or("dev")
        );
        return;
    }

    let mpv = player::mpv(&[]).expect("failed to create mpv");

    mpv.command("loadfile", &[&url]).expect("loadfile failed");

    // Ctrl-C needs no handling: libmpv leaves SIGINT alone, so the process just dies.
    loop {
        match mpv.wait_event(-1.0) {
            Some(Ok(Event::FileLoaded)) => {
                let title: String = mpv.get_property("media-title").unwrap_or_default();
                println!("playing: {title}");
            }
            Some(Ok(Event::EndFile(reason))) => {
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
            Some(Ok(Event::Shutdown)) => break,
            // libmpv2 reports a failed end-file as Err, dropping the event itself.
            Some(Err(e)) => {
                println!("ended: error ({e})");
                std::process::exit(1);
            }
            _ => {}
        }
    }
}
