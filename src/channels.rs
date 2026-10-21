//! Channel list from the R4 Supabase project, over plain PostgREST.
use serde::Deserialize;

const URL: &str = "https://oupgudlkspsmzkmeovlh.supabase.co/rest/v1/channels_with_tracks?select=slug,name,image,latest_track_at,track_count&order=latest_track_at.desc.nullslast&limit=200";
// The public anon key, the same one @radio4000/sdk ships.
pub const ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im91cGd1ZGxrc3BzbXprbWVvdmxoIiwicm9sZSI6ImFub24iLCJpYXQiOjE3MTM0NTIwNzQsImV4cCI6MjAyOTAyODA3NH0.KAbKFBChJHtxTmOZM2pdeppIyNbcnQkEgSi6RA7OQdo";

#[derive(Debug, Deserialize, PartialEq)]
pub struct Channel {
    pub slug: String,
    pub name: String,
    /// Cloudinary id of the avatar.
    pub image: Option<String>,
    pub latest_track_at: Option<String>,
    pub track_count: Option<u32>,
}

/// The avatar as a square PNG `px` pixels wide, the way r4-svelte builds it.
pub fn avatar_url(image: &str, px: u32) -> String {
    format!(
        "https://res.cloudinary.com/radio4000/image/upload/w_{px},h_{px},c_thumb,q_80/{image}.png"
    )
}

pub fn fetch() -> Result<Vec<Channel>, String> {
    let body = ureq::get(URL)
        .header("apikey", ANON_KEY)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse(&body)
}

fn parse(body: &str) -> Result<Vec<Channel>, String> {
    serde_json::from_str(body).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_postgrest_rows() {
        let rows = parse(r#"[{"slug":"wavs","name":"Wavs","latest_track_at":"2026-10-21T13:25:56+00:00","track_count":563},
            {"slug":"new","name":"New","latest_track_at":null,"track_count":null}]"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].slug, "wavs");
        assert_eq!(rows[1].latest_track_at, None);
    }
}
