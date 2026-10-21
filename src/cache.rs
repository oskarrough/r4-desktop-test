//! The channel list on disk, so the window has something to show before the network answers.
use crate::channels::Channel;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bump when the rule for which channels belong in the list changes.
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct Cache<C> {
    version: u32,
    channels: C,
}

/// `channels.json` in the app's one folder, beside the log.
fn path() -> Option<PathBuf> {
    Some(crate::app_dir()?.join("channels.json"))
}

/// The last list we saved, if there is one and it's current.
pub fn load() -> Option<Vec<Channel>> {
    decode(&std::fs::read_to_string(path()?).ok()?)
}

/// Best effort: a cache that cannot be written costs a slower next launch, nothing more.
pub fn save(channels: &[Channel]) {
    let _ = write(channels);
}

fn write(channels: &[Channel]) -> Option<()> {
    let path = path()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, encode(channels)?).ok()?;
    std::fs::rename(&tmp, &path).ok()
}

fn decode(body: &str) -> Option<Vec<Channel>> {
    let cache: Cache<Vec<Channel>> = serde_json::from_str(body).ok()?;
    (cache.version == VERSION).then_some(cache.channels)
}

fn encode(channels: &[Channel]) -> Option<String> {
    serde_json::to_string(&Cache {
        version: VERSION,
        channels,
    })
    .ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_list_from_before_the_youtube_rule_is_dropped() {
        let current = super::encode(&[]).unwrap();
        assert!(super::decode(&current).is_some());
        let legacy = r#"[{"slug":"cognitivetherapy","name":"Links only","track_count":1}]"#;
        assert!(super::decode(legacy).is_none());
        let older = r#"{"version":1,"channels":[]}"#;
        assert!(super::decode(older).is_none());
    }
}
