//! The channel list on disk, so the window has something to show before the network answers.
use crate::channels::Channel;
use serde::Deserialize;
use std::path::PathBuf;

/// Bump when the rule for which channels belong in the list changes, so a list
/// saved under the old rule is never shown. 2: only channels with a YouTube track.
const VERSION: u32 = 2;

#[derive(Deserialize)]
struct Cache {
    version: u32,
    channels: Vec<Channel>,
}

/// `channels.json` in the app's one folder, beside the log.
fn path() -> Option<PathBuf> {
    Some(crate::app_dir()?.join("channels.json"))
}

/// The last list we got, or `None` on a first run, an unreadable cache or one
/// saved by an older version.
pub fn load() -> Option<Vec<Channel>> {
    decode(&std::fs::read_to_string(path()?).ok()?)
}

fn decode(body: &str) -> Option<Vec<Channel>> {
    let cache: Cache = serde_json::from_str(body).ok()?;
    (cache.version == VERSION).then_some(cache.channels)
}

fn encode(channels: &[Channel]) -> Option<String> {
    serde_json::to_string(&serde_json::json!({ "version": VERSION, "channels": channels })).ok()
}

/// Best effort: a cache that cannot be written costs a slower next launch, nothing more.
pub fn save(channels: &[Channel]) {
    let Some(path) = path() else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Some(body) = encode(channels) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cache_and_log_share_one_folder() {
        let dir = crate::app_dir().unwrap();
        assert_eq!(super::path().unwrap().parent(), Some(dir.as_path()));
        #[cfg(target_os = "linux")]
        assert!(dir.ends_with("radio4000/desktop"), "{dir:?}");
    }

    #[test]
    fn a_saved_list_loads_back() {
        let channels =
            crate::channels::parse(r#"[{"slug":"wavs","name":"Wavs","track_count":3}]"#).unwrap();
        let loaded = super::decode(&super::encode(&channels).unwrap()).unwrap();
        assert_eq!(loaded[0].slug, "wavs");
    }

    #[test]
    fn a_list_from_before_the_youtube_rule_is_dropped() {
        // The old cache was a bare array, and may hold channels of links alone.
        let legacy = r#"[{"slug":"cognitivetherapy","name":"Links only","track_count":1}]"#;
        assert!(super::decode(legacy).is_none());
        let older = r#"{"version":1,"channels":[]}"#;
        assert!(super::decode(older).is_none());
    }
}
