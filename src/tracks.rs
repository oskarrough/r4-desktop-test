//! A channel's tracks from the R4 Supabase project, YouTube only.
use crate::channels::ANON_KEY;
use serde::{Deserialize, Deserializer};

const URL: &str = "https://oupgudlkspsmzkmeovlh.supabase.co/rest/v1/channel_tracks?select=title,description,url,provider,tags&order=created_at.desc";

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct Track {
    pub title: Option<String>,
    /// What the channel wrote about it.
    #[serde(default)]
    pub description: Option<String>,
    pub url: String,
    pub provider: Option<String>,
    /// The #hashtags in its description, without the #, lowercase.
    #[serde(default, deserialize_with = "lowercase")]
    pub tags: Vec<String>,
}

impl Track {
    pub fn tagged(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// The tags on these tracks with how many carry each, most used first, then alphabetically.
pub fn tags<'a>(list: impl IntoIterator<Item = &'a Track>) -> Vec<(&'a str, usize)> {
    let mut counts = std::collections::HashMap::new();
    for tag in list.into_iter().flat_map(|t| &t.tags) {
        *counts.entry(tag.as_str()).or_insert(0) += 1;
    }
    let mut tags: Vec<_> = counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    tags
}

fn lowercase<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let tags: Option<Vec<String>> = Option::deserialize(d)?;
    let mut tags: Vec<String> = tags
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.to_lowercase())
        .collect();
    tags.sort();
    tags.dedup();
    Ok(tags)
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

    #[test]
    fn counts_tags_most_used_first() {
        let rows = parse(
            r#"[{"title":"A","url":"a","provider":"youtube","tags":["Dub","video"]},
            {"title":"B","url":"b","provider":"youtube","tags":["dub"]},
            {"title":"C","url":"c","provider":"youtube","tags":null},
            {"title":"D","url":"d","provider":"youtube"}]"#,
        )
        .unwrap();
        assert!(rows[0].tagged("dub"));
        assert!(rows[2].tags.is_empty() && rows[3].tags.is_empty());
        assert_eq!(tags(&rows), [("dub", 2), ("video", 1)]);
    }
}
