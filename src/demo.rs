//! Fixed channels, tracks and a playing track, for screenshots that look the same every time.
//! `RADIO4000_DEMO=list|tracks|info|now|volume|search|idle radio4000` opens on that view, with no network and a player that stands still.
use crate::channels::Channel;
use crate::player::State;
use crate::tracks::Track;

pub struct Demo {
    pub channels: Vec<Channel>,
    pub tracks: Vec<Track>,
    pub state: State,
    /// Open on the tracklist.
    pub open: bool,
    /// Open on the channel's details.
    pub info: bool,
    /// Open on the now-playing view.
    pub now: bool,
    /// Hold the volume meter up, as if the volume just changed.
    pub volume: bool,
    /// Open on the search panel.
    pub search: bool,
}

pub fn from_env() -> Option<Demo> {
    let view = std::env::var("RADIO4000_DEMO")
        .ok()
        .filter(|v| !v.is_empty())?;
    let channel = |slug: &str, name: &str| Channel {
        slug: slug.into(),
        name: name.into(),
        // Real avatars, the one thing the demo fetches; the rest show their initial.
        image: match slug {
            "oskar" => Some("drz0qs9lgscyfdztr17t".into()),
            "wavs" => Some("q61pwrzvv7ty4crgqx2v".into()),
            "comballar" => Some("fewgs95wbzzk4kwhxt02".into()),
            "ko002" => Some("afeerseziomykfzwtr9e".into()),
            "belleville" => Some("fp4diwnoof0f8g4rllpb".into()),
            _ => None,
        },
        description: None,
        created_at: None,
        latest_track_at: None,
        track_count: None,
    };
    let oskar = Channel {
        description: Some("The channel of your wet dreams, an ode to perfume — for optimal peace and pleasure, shuffle this radio.".into()),
        created_at: Some("2014-09-20T11:49:05.028+00:00".into()),
        latest_track_at: Some("2026-10-18T12:13:18.205246+00:00".into()),
        track_count: Some(2648),
        ..channel("oskar", "Radio Oskar")
    };
    // Some with words from the channel, and the tags in them.
    let track = |title: &str| {
        let (title, words) = title.split_once(" | ").unwrap_or((title, ""));
        Track {
            title: Some(title.into()),
            description: (!words.is_empty()).then(|| words.into()),
            tags: words
                .split_whitespace()
                .filter_map(|w| w.strip_prefix('#'))
                .map(str::to_owned)
                .collect(),
            ..Track::default()
        }
    };
    let state = State {
        queue: Some("oskar".into()),
        channel: "Radio Oskar".into(),
        filter: None,
        index: Some(2),
        title: "Ry Cooder & Manuel Galbán – Drume Negrita".into(),
        paused: false,
        pos: 83.0,
        dur: Some(245.0),
        volume: 70.0,
        muted: false,
        error: None,
    };
    Some(Demo {
        channels: vec![
            channel("wavs", "Wavs Channel"),
            channel("comballar", "comballar"),
            channel("ko002", "ko002"),
            oskar,
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
            track("Alice Coltrane – Journey in Satchidananda | #spiritual #jazz"),
            track("Khruangbin – Maria También"),
            track("Ry Cooder & Manuel Galbán – Drume Negrita | hej far, håber det går godt!"),
            track("Mulatu Astatke – Yègellé Tezeta | #ethiojazz #jazz"),
            track("Arthur Russell – A Little Lost"),
            track("Os Mutantes – A Minha Menina"),
            track("Tinariwen – Cler Achel | #desert"),
            track("Broadcast – Come On Let's Go"),
            track("Ali Farka Touré & Toumani Diabaté – Ruby | #desert for long drives"),
            track("Stereolab – French Disko"),
            track("Ebo Taylor – Love and Death | #afrobeat #highlife"),
            track("Can – Vitamin C"),
            track("Nina Simone – Feeling Good"),
            track("William Onyeabor – Fantastic Man"),
            track("Cocteau Twins – Heaven or Las Vegas"),
            track("Fela Kuti – Water No Get Enemy | #afrobeat"),
            track("Hailu Mergia – Hari Meru Mela | #ethiojazz #jazz"),
            track("Joni Mitchell – Coyote"),
            track("Talking Heads – Once in a Lifetime"),
            track("Gal Costa – Pérola Negra"),
            track("Harold Budd – Afar"),
            track("Orchestra Baobab – Utrus Horas"),
            track("Sade – Cherish the Day"),
            track("Haruomi Hosono – Sports Men"),
            track("Bill Withers – Ain't No Sunshine"),
            track("Penguin Cafe Orchestra – Music for a Found Harmonium"),
        ],
        state: if view == "idle" {
            State::default()
        } else {
            state
        },
        open: matches!(view.as_str(), "tracks" | "info" | "now" | "volume"),
        info: view == "info",
        now: matches!(view.as_str(), "now" | "volume"),
        volume: view == "volume",
        search: view == "search",
    })
}
