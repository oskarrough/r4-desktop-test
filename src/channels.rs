//! Channel list from the R4 Supabase project, over plain PostgREST.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashSet;

const CHANNELS_URL: &str = "https://oupgudlkspsmzkmeovlh.supabase.co/rest/v1/channels_with_tracks?select=slug,name,description,image,created_at,latest_track_at,track_count&order=latest_track_at.desc.nullslast,slug.asc&track_count=gt.0";
const YOUTUBE_CHANNELS_URL: &str = "https://oupgudlkspsmzkmeovlh.supabase.co/rest/v1/channels?select=slug,channel_track!inner(tracks!inner())&channel_track.tracks.provider=eq.youtube&channel_track.limit=1&order=slug.asc";
/// Supabase caps a response at 1000 rows, so the list is paged.
const PAGE_SIZE: usize = 1000;
// The public anon key, the same one @radio4000/sdk ships.
pub const ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im91cGd1ZGxrc3BzbXprbWVvdmxoIiwicm9sZSI6ImFub24iLCJpYXQiOjE3MTM0NTIwNzQsImV4cCI6MjAyOTAyODA3NH0.KAbKFBChJHtxTmOZM2pdeppIyNbcnQkEgSi6RA7OQdo";

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Channel {
    pub slug: String,
    pub name: String,
    /// The channel's own words about itself.
    pub description: Option<String>,
    /// Cloudinary id of the avatar.
    pub image: Option<String>,
    pub created_at: Option<String>,
    pub latest_track_at: Option<String>,
    pub track_count: Option<u32>,
}

impl Channel {
    /// Its figures as the details panel shows them, each over a label: how many tracks, since when, the latest.
    pub fn stats(&self) -> Vec<(String, &'static str)> {
        let mut out = Vec::new();
        if let Some(n) = self.track_count {
            out.push((thousands(n), if n == 1 { "track" } else { "tracks" }));
        }
        if let Some(year) = self.created_at.as_deref().and_then(|d| d.get(..4)) {
            out.push((year.to_string(), "since"));
        }
        if let Some(d) = self.latest_track_at.as_deref().and_then(month) {
            out.push((d, "latest"));
        }
        out
    }
}

/// A channel's page on the web, without the scheme.
pub fn page(slug: &str) -> String {
    format!("radio4000.com/{slug}")
}

/// 2648 as "2,648".
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "2026-10-18T…" as "Oct 2026".
fn month(date: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let year = date.get(..4)?;
    let m: usize = date.get(5..7)?.parse().ok()?;
    Some(format!("{} {year}", MONTHS.get(m.checked_sub(1)?)?))
}

/// The avatar as a square PNG `px` pixels wide, the way r4-svelte builds it.
pub fn avatar_url(image: &str, px: u32) -> String {
    format!(
        "https://res.cloudinary.com/radio4000/image/upload/w_{px},h_{px},c_thumb,q_80/{image}.png"
    )
}

/// Every channel with something to play, in latest-track order.
pub fn fetch() -> Result<Vec<Channel>, String> {
    let mut channels: Vec<Channel> = fetch_all(CHANNELS_URL)?;
    let youtube = youtube_slugs()?;
    channels.retain(|c| playable(c, &youtube));
    Ok(channels)
}

fn youtube_slugs() -> Result<HashSet<String>, String> {
    #[derive(Deserialize)]
    struct Row {
        slug: String,
    }
    let rows: Vec<Row> = fetch_all(YOUTUBE_CHANNELS_URL)?;
    Ok(rows.into_iter().map(|r| r.slug).collect())
}

fn fetch_all<T: DeserializeOwned>(url: &str) -> Result<Vec<T>, String> {
    let mut rows = Vec::new();
    loop {
        let page: Vec<T> = fetch_page(url, rows.len())?;
        let last = page.len() < PAGE_SIZE;
        rows.extend(page);
        if last {
            return Ok(rows);
        }
    }
}

fn fetch_page<T: DeserializeOwned>(url: &str, offset: usize) -> Result<Vec<T>, String> {
    let body = ureq::get(url)
        .query("limit", PAGE_SIZE.to_string())
        .query("offset", offset.to_string())
        .header("apikey", ANON_KEY)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// track_count counts every provider, but only YouTube tracks play.
fn playable(c: &Channel, youtube: &HashSet<String>) -> bool {
    c.track_count.is_some_and(|n| n > 0) && youtube.contains(&c.slug)
}

#[cfg(test)]
pub(crate) fn parse(body: &str) -> Result<Vec<Channel>, String> {
    serde_json::from_str(body).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playable_needs_a_youtube_track() {
        let rows = parse(
            r#"[{"slug":"wavs","name":"Wavs","track_count":3},
            {"slug":"zero","name":"Zero","track_count":0},
            {"slug":"null","name":"Null","track_count":null},
            {"slug":"cognitivetherapy","name":"Links only","track_count":1}]"#,
        )
        .unwrap();
        let youtube = ["wavs", "zero", "null"].map(String::from).into();
        let kept: Vec<_> = rows
            .iter()
            .filter(|c| playable(c, &youtube))
            .map(|c| c.slug.as_str())
            .collect();
        assert_eq!(kept, ["wavs"]);
    }

    #[test]
    fn states_its_figures() {
        let rows = parse(r#"[{"slug":"oskar","name":"Radio Oskar","description":"Perfume.","created_at":"2014-09-20T11:49:05.028+00:00",
            "latest_track_at":"2026-10-18T12:13:18+00:00","track_count":2648}]"#).unwrap();
        let c = &rows[0];
        assert_eq!(c.description.as_deref(), Some("Perfume."));
        let stats = c.stats();
        assert_eq!(
            stats,
            vec![
                ("2,648".to_string(), "tracks"),
                ("2014".to_string(), "since"),
                ("Oct 2026".to_string(), "latest"),
            ]
        );
        assert_eq!(page(&c.slug), "radio4000.com/oskar");
        assert_eq!(thousands(1), "1");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
