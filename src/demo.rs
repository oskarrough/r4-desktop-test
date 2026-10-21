//! Fixed channels, tracks and a playing track, for screenshots that look the same every time.
//! `RADIO4000_DEMO=list|tracks|now|idle radio4000` opens on that view, with no network and a player that stands still.
use crate::channels::Channel;
use crate::player::State;
use crate::tracks::Track;

pub struct Demo {
    pub channels: Vec<Channel>,
    pub tracks: Vec<Track>,
    pub state: State,
    /// Open on the tracklist.
    pub open: bool,
    /// Open on the now-playing view.
    pub now: bool,
}

pub fn from_env() -> Option<Demo> {
    let view = std::env::var("RADIO4000_DEMO")
        .ok()
        .filter(|v| !v.is_empty())?;
    let channel = |slug: &str, name: &str| Channel {
        slug: slug.into(),
        name: name.into(),
        latest_track_at: None,
        track_count: None,
    };
    let track = |title: &str| Track {
        title: Some(title.into()),
        url: String::new(),
        provider: None,
    };
    let state = State {
        queue: Some("oskar".into()),
        channel: "Radio Oskar".into(),
        index: Some(2),
        title: "Ry Cooder & Manuel Galbán – Drume Negrita".into(),
        paused: false,
        pos: 83.0,
        dur: Some(245.0),
        error: None,
    };
    Some(Demo {
        channels: vec![
            channel("wavs", "Wavs Channel"),
            channel("comballar", "comballar"),
            channel("ko002", "ko002"),
            channel("oskar", "Radio Oskar"),
            channel("belleville", "Radio Belleville"),
            channel("sempre", "SEMPRELIGADO FM 📻"),
            channel("zero", "Zer0's Radio Part II"),
            channel("aude", "Aude Sapere"),
            channel("alice", "alice3000"),
            channel("shark", "🎇🎉 PARTY SHARK 🎉🎇"),
            channel("solsbury", "Solsbury hill"),
            channel(
                "padi",
                "The PADI IDC Gili Trawangan – Gili Islands – Indonesia",
            ),
            channel("cocoa", "Cocoa Beach Eco Tours"),
        ],
        tracks: vec![
            track("Alice Coltrane – Journey in Satchidananda"),
            track("Khruangbin – Maria También"),
            track("Ry Cooder & Manuel Galbán – Drume Negrita"),
            track("Mulatu Astatke – Yègellé Tezeta"),
            track("Arthur Russell – A Little Lost"),
            track("Os Mutantes – A Minha Menina"),
            track("Tinariwen – Cler Achel"),
            track("Broadcast – Come On Let's Go"),
            track("Ali Farka Touré & Toumani Diabaté – Ruby"),
            track("Stereolab – French Disko"),
            track("Ebo Taylor – Love and Death"),
        ],
        state: if view == "idle" {
            State::default()
        } else {
            state
        },
        open: view == "tracks" || view == "now",
        now: view == "now",
    })
}
