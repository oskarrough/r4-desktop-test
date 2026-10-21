//! Plays a channel's tracklist through mpv's own playlist, which gives us next, previous and skipping past tracks that fail to load.
//! mpv lives on its own thread: the window sends commands and reads the latest state, so it never waits on mpv.
use libmpv2::Mpv;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// How often the thread looks at mpv when no command arrives; the seek bar moves at this pace.
const POLL: Duration = Duration::from_millis(200);

pub fn mpv(extra: &[(&str, &str)]) -> libmpv2::Result<Mpv> {
    Mpv::with_initializer(|init| {
        init.set_option("vid", "no")?;
        init.set_option("ytdl", "yes")?;
        init.set_option("ytdl-format", "bestaudio/best")?;
        init.set_option("terminal", "yes")?;
        init.set_option("msg-level", "all=warn")?;
        init.set_option("audio-client-name", "radio4000")?;
        for (k, v) in extra {
            init.set_option(k, *v)?;
        }
        Ok(())
    })
}

pub struct Track {
    pub url: String,
    pub title: String,
}

pub enum Command {
    /// Replace the playlist with `tracks` unless `queue` is already loaded, then start at `index`.
    Play {
        queue: String,
        channel: String,
        tracks: Vec<Track>,
        index: usize,
    },
    Toggle,
    Next,
    Prev,
    /// To this many seconds into the track.
    Seek(f64),
    /// By this many seconds, back when negative.
    SeekBy(f64),
    /// Set the volume, 0 to 100, and unmute.
    Volume(f64),
    ToggleMute,
}

/// What the player is doing, as of the thread's last look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// Slug of the loaded channel.
    pub queue: Option<String>,
    pub channel: String,
    /// Position of the current track in the channel's tracklist.
    pub index: Option<usize>,
    pub title: String,
    pub paused: bool,
    pub pos: f64,
    /// Unknown while yt-dlp resolves the track.
    pub dur: Option<f64>,
    /// 0 to 100.
    pub volume: f64,
    pub muted: bool,
    pub error: Option<String>,
}

impl State {
    pub fn loading(&self) -> bool {
        self.index.is_some() && self.dur.is_none()
    }
}

pub struct Player {
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<State>,
    pub state: State,
}

impl Player {
    /// Starts mpv on its own thread. `changed` runs there whenever the state moves, to wake the window.
    pub fn spawn(
        extra: &'static [(&'static str, &'static str)],
        changed: impl Fn() + Send + 'static,
    ) -> Self {
        let (tx, commands) = mpsc::channel();
        let (states, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let send = |s: &State| {
                let _ = states.send(s.clone());
                changed();
            };
            let mpv = match mpv(extra) {
                Ok(m) => m,
                Err(e) => {
                    send(&State {
                        error: Some(e.to_string()),
                        ..State::default()
                    });
                    return;
                }
            };
            let mut engine = Engine {
                mpv,
                titles: Vec::new(),
                state: State::default(),
            };
            loop {
                match commands.recv_timeout(POLL) {
                    Ok(c) => engine.run(c),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                // Nobody reads mpv's events, so drop them before its queue fills.
                while engine.mpv.wait_event(0.0).is_some() {}
                if engine.look() {
                    send(&engine.state);
                }
            }
        });
        Player {
            tx,
            rx,
            state: State::default(),
        }
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    /// Takes in what the thread reported since the last frame.
    pub fn update(&mut self) {
        while let Ok(s) = self.rx.try_recv() {
            self.state = s;
        }
    }
}

/// The thread's side: mpv and what it knows about the loaded channel.
struct Engine {
    mpv: Mpv,
    titles: Vec<String>,
    state: State,
}

impl Engine {
    fn run(&mut self, c: Command) {
        let mpv = &self.mpv;
        match c {
            Command::Play {
                queue,
                channel,
                tracks,
                index,
            } => {
                if self.state.queue.as_deref() != Some(&queue) {
                    let _ = mpv.command("playlist-clear", &[]);
                    let _ = mpv.command("stop", &[]);
                    for t in &tracks {
                        let _ = mpv.command("loadfile", &[&t.url, "append"]);
                    }
                    self.titles = tracks.into_iter().map(|t| t.title).collect();
                    self.state.queue = Some(queue);
                    self.state.channel = channel;
                }
                let _ = mpv.command("playlist-play-index", &[&index.to_string()]);
                let _ = mpv.set_property("pause", false);
            }
            Command::Toggle => {
                let _ = mpv.command("cycle", &["pause"]);
            }
            Command::Next => {
                let _ = mpv.command("playlist-next", &[]);
            }
            Command::Prev => {
                let _ = mpv.command("playlist-prev", &[]);
            }
            Command::Seek(secs) => {
                let _ = mpv.command("seek", &[&secs.to_string(), "absolute"]);
            }
            Command::SeekBy(secs) => {
                let _ = mpv.command("seek", &[&secs.to_string(), "relative"]);
            }
            // The audio server's own stream volume and mute act at once; mpv's would wait out its audio buffer.
            Command::Volume(v) => {
                let v = v.clamp(0.0, 100.0);
                if mpv.set_property("ao-volume", v).is_err() {
                    let _ = mpv.set_property("volume", v);
                }
                if mpv.set_property("ao-mute", false).is_err() {
                    let _ = mpv.set_property("mute", false);
                }
            }
            Command::ToggleMute => {
                if mpv.command("cycle", &["ao-mute"]).is_err() {
                    let _ = mpv.command("cycle", &["mute"]);
                }
            }
        }
    }

    /// Reads mpv into the state; true if anything changed.
    fn look(&mut self) -> bool {
        let mpv = &self.mpv;
        let before = self.state.clone();
        let s = &mut self.state;
        s.index = s
            .queue
            .as_ref()
            .and_then(|_| mpv.get_property::<i64>("playlist-pos").ok())
            .and_then(|i| usize::try_from(i).ok());
        s.title = s
            .index
            .and_then(|i| self.titles.get(i).cloned())
            .unwrap_or_default();
        s.paused = mpv.get_property("pause").unwrap_or(false);
        s.pos = mpv.get_property("time-pos").unwrap_or(0.0);
        s.dur = mpv.get_property("duration").ok();
        s.volume = mpv
            .get_property("ao-volume")
            .or_else(|_| mpv.get_property("volume"))
            .unwrap_or(100.0);
        s.muted = mpv
            .get_property("ao-mute")
            .or_else(|_| mpv.get_property("mute"))
            .unwrap_or(false);
        *s != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let bad = dir.join("missing.wav");
        let track = |p: &std::path::Path, title: &str| Track {
            url: p.to_str().unwrap().to_string(),
            title: title.to_string(),
        };
        // Pause at the end, so the good track stays current long enough to see.
        let mut p = Player::spawn(&[("ao", "null"), ("keep-open", "yes")], || {});
        p.send(Command::Play {
            queue: "c".into(),
            channel: "C".into(),
            tracks: vec![track(&bad, "bad"), track(&good, "good")],
            index: 0,
        });
        let start = std::time::Instant::now();
        while !(p.state.index == Some(1) && p.state.dur.is_some()) {
            assert!(
                start.elapsed().as_secs() < 10,
                "never loaded the good track"
            );
            std::thread::sleep(Duration::from_millis(50));
            p.update();
        }
        assert_eq!(p.state.title, "good");
        assert_eq!(p.state.channel, "C");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
