//! The channel list on disk, so the window has something to show before the network answers.
use crate::channels::Channel;
use std::path::PathBuf;

/// `$XDG_CACHE_HOME/radio4000/channels.json`, falling back to `%LOCALAPPDATA%` and then `~/.cache`.
fn path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(dir.join("radio4000").join("channels.json"))
}

/// The last list we got, or `None` on a first run or an unreadable cache.
pub fn load() -> Option<Vec<Channel>> {
    let body = std::fs::read_to_string(path()?).ok()?;
    let channels = serde_json::from_str(&body).ok()?;
    Some(channels)
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
    let Ok(body) = serde_json::to_string(channels) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}
