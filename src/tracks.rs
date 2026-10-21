//! A channel's tracks from the R4 Supabase project, YouTube only.
use crate::channels::ANON_KEY;
use serde::Deserialize;

const URL: &str = "https://oupgudlkspsmzkmeovlh.supabase.co/rest/v1/channel_tracks?select=title,url,provider&order=created_at.desc";

#[derive(Debug, Deserialize, PartialEq)]
pub struct Track {
    pub title: Option<String>,
    pub url: String,
    pub provider: Option<String>,
}

pub fn fetch(slug: &str) -> Result<Vec<Track>, String> {
    let body = ureq::get(URL)
        .query("slug", format!("eq.{slug}"))
        .header("apikey", ANON_KEY)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse(&body)
}

/// Keeps only YouTube tracks; SoundCloud and the rest are dropped here.
fn parse(body: &str) -> Result<Vec<Track>, String> {
    let rows: Vec<Track> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|t| t.provider.as_deref() == Some("youtube"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_youtube() {
        let rows = parse(
            r#"[{"title":"A","url":"https://www.youtube.com/watch?v=a","provider":"youtube"},
            {"title":"B","url":"https://soundcloud.com/x/b","provider":"soundcloud"},
            {"title":null,"url":"https://example.com/c","provider":null}]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title.as_deref(), Some("A"));
    }
}
