//! Playback spike: play the audio of one URL via libmpv + yt-dlp, exit when it ends.
use libmpv2::{Mpv, events::Event};

fn main() {
    let Some(url) = std::env::args().nth(1) else {
        eprintln!("usage: radio4000 <url>");
        std::process::exit(2);
    };

    let mpv = Mpv::with_initializer(|init| {
        init.set_option("vid", "no")?;
        init.set_option("ytdl", "yes")?;
        init.set_option("ytdl-format", "bestaudio/best")?;
        init.set_option("terminal", "yes")?;
        init.set_option("msg-level", "all=warn")?;
        Ok(())
    })
    .expect("failed to create mpv");

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
