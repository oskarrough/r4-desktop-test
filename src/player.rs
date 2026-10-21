//! Plays a channel's tracklist through mpv's own playlist, which gives us next, previous and skipping past tracks that fail to load.
use libmpv2::Mpv;

pub fn mpv() -> libmpv2::Result<Mpv> {
    Mpv::with_initializer(|init| {
        init.set_option("vid", "no")?;
        init.set_option("ytdl", "yes")?;
        init.set_option("ytdl-format", "bestaudio/best")?;
        init.set_option("terminal", "yes")?;
        init.set_option("msg-level", "all=warn")?;
        Ok(())
    })
}

pub struct Player {
    pub mpv: Mpv,
    /// Identifies the loaded playlist, so the tracklist knows whether it is the one playing.
    pub queue: Option<String>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        Ok(Player {
            mpv: mpv().map_err(|e| e.to_string())?,
            queue: None,
        })
    }

    /// Replaces the playlist with `urls` and starts at `index`.
    pub fn play(&mut self, queue: &str, urls: &[&str], index: usize) {
        if self.queue.as_deref() != Some(queue) {
            let _ = self.mpv.command("playlist-clear", &[]);
            let _ = self.mpv.command("stop", &[]);
            for url in urls {
                let _ = self.mpv.command("loadfile", &[url, "append"]);
            }
            self.queue = Some(queue.to_string());
        }
        let _ = self
            .mpv
            .command("playlist-play-index", &[&index.to_string()]);
        let _ = self.mpv.set_property("pause", false);
    }

    /// Index of the current track in the playlist, if one is playing.
    pub fn current(&self) -> Option<usize> {
        self.queue.as_ref()?;
        let pos: i64 = self.mpv.get_property("playlist-pos").ok()?;
        usize::try_from(pos).ok()
    }

    pub fn paused(&self) -> bool {
        self.mpv.get_property("pause").unwrap_or(false)
    }

    pub fn toggle_pause(&self) {
        let _ = self.mpv.command("cycle", &["pause"]);
    }

    /// Seconds into the current track and its length, once mpv knows both.
    pub fn progress(&self) -> Option<(f64, f64)> {
        let pos: f64 = self.mpv.get_property("time-pos").ok()?;
        let dur: f64 = self.mpv.get_property("duration").ok()?;
        Some((pos, dur))
    }

    pub fn seek(&self, secs: f64) {
        let _ = self.mpv.command("seek", &[&secs.to_string(), "absolute"]);
    }

    pub fn next(&self) {
        let _ = self.mpv.command("playlist-next", &[]);
    }

    pub fn prev(&self) {
        let _ = self.mpv.command("playlist-prev", &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libmpv2::events::Event;

    /// One second of silent 8 kHz mono WAV.
    fn silence(path: &std::path::Path) {
        let n: u32 = 8000;
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + n).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(n.to_le_bytes());
        b.extend(n.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(8u16.to_le_bytes());
        b.extend(b"data");
        b.extend(n.to_le_bytes());
        b.extend(std::iter::repeat_n(128u8, n as usize));
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn skips_a_track_that_fails_to_load() {
        let dir = std::env::temp_dir().join(format!("r4-player-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.wav");
        silence(&good);
        let mut p = Player::new().unwrap();
        p.mpv.set_property("ao", "null").unwrap();
        let bad = dir.join("missing.wav");
        p.play("c", &[bad.to_str().unwrap(), good.to_str().unwrap()], 0);
        let start = std::time::Instant::now();
        loop {
            assert!(
                start.elapsed().as_secs() < 10,
                "never loaded the good track"
            );
            if let Some(Ok(Event::FileLoaded)) = p.mpv.wait_event(1.0) {
                break;
            }
        }
        assert_eq!(p.current(), Some(1));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
