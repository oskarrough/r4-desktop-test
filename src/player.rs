//! Plays a channel's tracklist through mpv's own playlist, which gives us next, previous and skipping past tracks that fail to load.
//! mpv lives on its own thread: the window sends commands and reads the latest state, so it never waits on mpv.
use libmpv2::Mpv;
use libmpv2::events::Event;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// How often the thread looks at mpv when no command arrives; the seek bar moves at this pace.
const POLL: Duration = Duration::from_millis(200);

/// An mpv whose warnings come as events for [`log_event`], so they land in the log file rather than only the terminal.
pub fn mpv(extra: &[(&str, &str)]) -> libmpv2::Result<Mpv> {
    let mpv = Mpv::with_initializer(|init| {
        init.set_option("vid", "no")?;
        init.set_option("ytdl", "yes")?;
        init.set_option("ytdl-format", "bestaudio/best")?;
        crate::playback_tools::configure(&init)?;
        // A stalled stream fails and gets skipped after this many seconds, rather than mpv's default minute.
        init.set_option("network-timeout", "15")?;
        init.set_option("audio-client-name", "radio4000")?;
        for (k, v) in extra {
            init.set_option(k, *v)?;
        }
        Ok(())
    })?;
    // SAFETY: the handle is live for as long as `mpv`, and the level is a C string.
    unsafe { libmpv2_sys::mpv_request_log_messages(mpv.ctx.as_ptr(), c"warn".as_ptr()) };
    Ok(mpv)
}

/// Logs mpv's warnings and errors, and each track that loads or fails.
pub fn log_event(mpv: &Mpv, event: &libmpv2::Result<Event>) {
    match event {
        Ok(Event::LogMessage {
            prefix,
            level,
            text,
            ..
        }) => {
            let level = if matches!(*level, "fatal" | "error") {
                log::Level::Error
            } else {
                log::Level::Warn
            };
            log::log!(target: "mpv", level, "{prefix}: {}", text.trim_end());
        }
        Ok(Event::FileLoaded) => {
            let title: String = mpv.get_property("media-title").unwrap_or_default();
            log::info!("playing {title}");
        }
        // libmpv2 reports a failed end-file as Err, dropping the event itself.
        Err(e) => log::warn!("track failed: {e}"),
        _ => {}
    }
}

#[derive(Clone)]
pub struct Track {
    pub url: String,
    pub title: String,
}

pub enum Command {
    /// Shuffle the `pool` of `tracks` into the playlist unless `queue` and `filter` are loaded, then start at `index`.
    Play {
        queue: String,
        filter: Option<String>,
        channel: String,
        tracks: Vec<Track>,
        pool: Vec<usize>,
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
    /// The loaded channel's filter, as typed.
    pub filter: Option<String>,
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

    /// The channel's name and filter.
    pub fn station(&self) -> String {
        match &self.filter {
            Some(filter) => format!("{} · {filter}", self.channel),
            None => self.channel.clone(),
        }
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
                order: Vec::new(),
                state: State::default(),
            };
            loop {
                match commands.recv_timeout(POLL) {
                    Ok(c) => engine.run(c),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                // Read mpv's events into the log before its queue fills.
                while let Some(event) = engine.mpv.wait_event(0.0) {
                    log_event(&engine.mpv, &event);
                }
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
    /// The channel's track at each place in mpv's playlist.
    order: Vec<usize>,
    state: State,
}

impl Engine {
    fn run(&mut self, c: Command) {
        let mpv = &self.mpv;
        match c {
            Command::Play {
                queue,
                filter,
                channel,
                tracks,
                pool,
                index,
            } => {
                if self.state.queue.as_deref() != Some(&queue) || self.state.filter != filter {
                    let _ = mpv.command("playlist-clear", &[]);
                    let _ = mpv.command("stop", &[]);
                    let first = pool.iter().position(|&i| i == index).unwrap_or(0);
                    self.order = shuffle(pool.len(), first, random())
                        .into_iter()
                        .filter_map(|k| pool.get(k).copied())
                        .filter(|&i| i < tracks.len())
                        .collect();
                    for &i in &self.order {
                        let _ = mpv.command("loadfile", &[&tracks[i].url, "append"]);
                    }
                    self.titles = tracks.into_iter().map(|t| t.title).collect();
                    self.state.queue = Some(queue);
                    self.state.filter = filter;
                    self.state.channel = channel;
                }
                let at = self.order.iter().position(|&i| i == index).unwrap_or(0);
                let _ = mpv.command("playlist-play-index", &[&at.to_string()]);
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
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|at| self.order.get(at).copied());
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

/// A different number on every call, enough to shuffle by.
pub fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// A track to start a channel of `n` on, other than `not` when there's a choice.
pub fn pick(n: usize, not: Option<usize>, seed: u64) -> usize {
    match not.filter(|&c| c < n && n > 1) {
        Some(c) => {
            let i = (seed % (n as u64 - 1)) as usize;
            if i >= c { i + 1 } else { i }
        }
        None => (seed % n.max(1) as u64) as usize,
    }
}

/// The `n` tracks in random order, `first` leading.
fn shuffle(n: usize, first: usize, mut seed: u64) -> Vec<usize> {
    let mut rest: Vec<usize> = (0..n).filter(|&i| i != first).collect();
    for i in (1..rest.len()).rev() {
        // xorshift64; a zero seed would stay zero.
        seed = seed.max(1);
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        rest.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    std::iter::once(first)
        .filter(|&f| f < n)
        .chain(rest)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffles_every_track_once_from_the_chosen_one() {
        let orders: Vec<Vec<usize>> = (1..=5).map(|seed| shuffle(10, 3, seed)).collect();
        for o in &orders {
            assert_eq!(o[0], 3);
            let mut sorted = o.clone();
            sorted.sort();
            assert_eq!(sorted, (0..10).collect::<Vec<_>>());
        }
        assert!(orders.windows(2).any(|w| w[0] != w[1]), "never shuffled");
        assert_eq!(shuffle(1, 0, 7), vec![0]);
        assert!(shuffle(0, 0, 7).is_empty());
    }

    #[test]
    fn playing_a_channel_again_starts_elsewhere() {
        for seed in 0..50 {
            assert_ne!(pick(5, Some(2), seed), 2);
            assert!(pick(5, Some(2), seed) < 5);
            assert!(pick(5, None, seed) < 5);
        }
        assert_eq!(pick(1, Some(0), 9), 0);
        assert_eq!(pick(0, None, 9), 0);
    }

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
            filter: None,
            channel: "C".into(),
            tracks: vec![track(&bad, "bad"), track(&good, "good")],
            pool: vec![0, 1],
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

    #[test]
    fn reports_the_chosen_track_through_the_shuffle() {
        let dir = std::env::temp_dir().join(format!("r4-shuffle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tracks: Vec<_> = (0..6)
            .map(|i| {
                let path = dir.join(format!("{i}.wav"));
                silence(&path);
                Track {
                    url: path.to_str().unwrap().to_string(),
                    title: format!("t{i}"),
                }
            })
            .collect();
        let mut p = Player::spawn(&[("ao", "null"), ("keep-open", "yes")], || {});
        p.send(Command::Play {
            queue: "c".into(),
            filter: None,
            channel: "C".into(),
            tracks: tracks.clone(),
            pool: (0..6).collect(),
            index: 4,
        });
        let start = std::time::Instant::now();
        while p.state.dur.is_none() {
            assert!(start.elapsed().as_secs() < 10, "never loaded");
            std::thread::sleep(Duration::from_millis(50));
            p.update();
        }
        assert_eq!(p.state.index, Some(4));
        assert_eq!(p.state.title, "t4");

        // A filter plays only its pool, with channel indices.
        p.send(Command::Play {
            queue: "c".into(),
            filter: Some("#dub".into()),
            channel: "C".into(),
            tracks,
            pool: vec![1, 3],
            index: 3,
        });
        p.send(Command::Next);
        let start = std::time::Instant::now();
        while !(p.state.index == Some(1) && p.state.dur.is_some()) {
            assert!(
                start.elapsed().as_secs() < 10,
                "never moved within the filter"
            );
            std::thread::sleep(Duration::from_millis(50));
            p.update();
        }
        assert_eq!(p.state.filter.as_deref(), Some("#dub"));
        assert_eq!(p.state.station(), "C · #dub");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
