//! The desktop's media controls: media keys and headset buttons reach the player, and the system's now-playing widget shows what's on.
use crate::player::{Command as Play, State};
use fastframe_now_playing::{Controls, Playback, State as Shown, Track};
use std::time::{Duration, Instant};

pub use fastframe_now_playing::{App, Command, NowPlaying};

/// `radio4000` on the bus, or `radio4000.instance_<name>` for a [`crate::instance`], as `playerctl -p` names it.
pub fn app() -> App {
    let bus_name = match crate::instance() {
        Some(name) => format!("radio4000.instance_{name}"),
        None => "radio4000".to_owned(),
    };
    App {
        desktop_entry: "radio4000".to_owned(),
        ..App::new(&bus_name, "Radio4000")
    }
}

/// The id the widget knows the playing track by: channel and position, so a seek meant for an earlier track can be told apart.
pub fn track_id(s: &State) -> Option<String> {
    Some(format!("{}/{}", s.queue.as_deref()?, s.index?))
}

/// What the widget shows for the player's state: the track as title, the channel as artist, its avatar as cover.
pub fn shown(s: &State, avatar: Option<String>) -> Shown {
    let Some(id) = track_id(s) else {
        return Shown {
            controls: Controls {
                next: false,
                previous: false,
                seek: false,
                ..Controls::default()
            },
            ..Shown::default()
        };
    };
    Shown {
        playback: if s.paused || s.loading() {
            Playback::Paused
        } else {
            Playback::Playing
        },
        position: Duration::from_secs_f64(s.pos.max(0.0)),
        volume: Some(if s.muted { 0.0 } else { s.volume / 100.0 }),
        controls: Controls {
            seek: s.dur.is_some(),
            ..Controls::default()
        },
        track: Some(Track {
            id,
            title: s.title.clone(),
            artists: vec![s.channel.clone()],
            duration: s.dur.map(|d| Duration::from_secs_f64(d.max(0.0))),
            art_url: avatar,
            ..Track::default()
        }),
        ..Shown::default()
    }
}

/// A seek sent to mpv, waiting for mpv to report where it landed so the widget can jump there rather than glide.
pub struct Seek {
    to: f64,
    at: Instant,
}

impl Seek {
    /// The seek `c` makes from `s`, if it is one.
    pub fn new(s: &State, c: &Play) -> Option<Self> {
        let to = match *c {
            Play::Seek(to) => to,
            Play::SeekBy(by) => s.pos + by,
            _ => return None,
        };
        let to = to.clamp(0.0, s.dur.unwrap_or(f64::MAX));
        Some(Seek {
            to,
            at: Instant::now(),
        })
    }

    /// Whether mpv is there by `pos`, or has given up on getting there.
    pub fn landed(&self, pos: f64) -> bool {
        (pos - self.to).abs() < 1.5 || self.at.elapsed() > Duration::from_secs(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> State {
        State {
            queue: Some("oskar".into()),
            channel: "Radio Oskar".into(),
            index: Some(3),
            title: "Ali Farka Touré - Diaraby".into(),
            pos: 83.0,
            dur: Some(312.0),
            volume: 80.0,
            ..State::default()
        }
    }

    #[test]
    fn paused_while_loading_or_paused() {
        let mut s = playing();
        s.dur = None;
        assert_eq!(shown(&s, None).playback, Playback::Paused);
        assert!(!shown(&s, None).controls.seek);
        s = playing();
        s.paused = true;
        assert_eq!(shown(&s, None).playback, Playback::Paused);
    }

    #[test]
    fn nothing_loaded_is_stopped() {
        let shown = shown(&State::default(), None);
        assert_eq!(shown.playback, Playback::Stopped);
        assert!(shown.track.is_none());
        assert!(!shown.controls.next);
        // Play still answers: it starts the open channel.
        assert!(shown.controls.play);
    }

    #[test]
    fn a_seek_lands_where_it_was_sent() {
        let s = playing();
        let by = Seek::new(&s, &Play::SeekBy(10.0)).unwrap();
        assert!(!by.landed(83.2));
        assert!(by.landed(93.1));
        let to = Seek::new(&s, &Play::Seek(30.0)).unwrap();
        assert!(!to.landed(83.0));
        assert!(to.landed(30.0));
        // Back past the start lands at zero.
        assert!(Seek::new(&s, &Play::SeekBy(-200.0)).unwrap().landed(0.0));
        assert!(Seek::new(&s, &Play::Next).is_none());
    }

    #[test]
    fn muted_shows_silence() {
        let mut s = playing();
        s.muted = true;
        assert_eq!(shown(&s, None).volume, Some(0.0));
    }
}
