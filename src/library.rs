//! Channels and tracks as they arrive, with a cached channel list while the network refreshes.
use crate::channels::{self, Channel};
use crate::tracks::{self, Track};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};

pub struct Library {
    channels: Option<Result<Vec<Channel>, String>>,
    channel_rx: Option<mpsc::Receiver<Result<Vec<Channel>, String>>>,
    /// Every channel requested so far, including failures, so each is fetched only once.
    tracks: HashMap<String, Tracks>,
    /// Fixed libraries never start network requests.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

struct Tracks {
    list: Option<Result<Vec<Track>, String>>,
    rx: Option<mpsc::Receiver<Result<Vec<Track>, String>>>,
}

impl Library {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let notify = Arc::clone(&wake);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // The cached list paints first, the network corrects it a moment later.
            if let Some(cached) = crate::cache::load() {
                let _ = tx.send(Ok(cached));
                notify();
            }
            let fresh = channels::fetch();
            match &fresh {
                Ok(list) => crate::cache::save(list),
                Err(e) => log::warn!("could not load channels: {e}"),
            }
            let _ = tx.send(fresh);
            notify();
        });
        Self {
            channels: None,
            channel_rx: Some(rx),
            tracks: HashMap::new(),
            wake: Some(wake),
        }
    }

    /// Demo and test data, with no background requests or receivers.
    pub fn fixed(channels: Vec<Channel>, tracks: HashMap<String, Vec<Track>>) -> Self {
        Self {
            channels: Some(Ok(channels)),
            channel_rx: None,
            tracks: tracks
                .into_iter()
                .map(|(slug, list)| {
                    (
                        slug,
                        Tracks {
                            list: Some(Ok(list)),
                            rx: None,
                        },
                    )
                })
                .collect(),
            wake: None,
        }
    }

    pub fn poll_updates(&mut self) {
        // One channel result per frame lets the cached list paint before a queued refresh.
        if let Some(rx) = &self.channel_rx
            && let Ok(result) = rx.try_recv()
        {
            // A failed refresh is no reason to throw away a list that works.
            if !(result.is_err() && matches!(self.channels, Some(Ok(_)))) {
                self.channels = Some(result);
            }
        }
        for tracks in self.tracks.values_mut() {
            if let Some(rx) = &tracks.rx
                && let Ok(result) = rx.try_recv()
            {
                tracks.list = Some(result);
                tracks.rx = None;
            }
        }
    }

    /// Starts at most one request per slug, retaining both successful and failed results.
    pub fn request_tracks(&mut self, slug: &str) {
        if self.tracks.contains_key(slug) {
            return;
        }
        let Some(wake) = self.wake.as_ref().map(Arc::clone) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let slug = slug.to_string();
        self.tracks.insert(
            slug.clone(),
            Tracks {
                list: None,
                rx: Some(rx),
            },
        );
        std::thread::spawn(move || {
            let tracks = tracks::fetch(&slug);
            if let Err(e) = &tracks {
                log::warn!("could not load the tracks of {slug}: {e}");
            }
            let _ = tx.send(tracks);
            wake();
        });
    }

    /// Loading is `None`; a loaded list or an error borrows its stored result.
    pub fn channels(&self) -> Option<Result<&[Channel], &str>> {
        self.channels
            .as_ref()
            .map(|result| result.as_deref().map_err(String::as_str))
    }

    /// Unrequested and loading slugs are `None`, including missing slugs in fixed data.
    pub fn tracks(&self, slug: &str) -> Option<Result<&[Track], &str>> {
        self.tracks
            .get(slug)?
            .list
            .as_ref()
            .map(|result| result.as_deref().map_err(String::as_str))
    }

    pub fn find(&self, slug: &str) -> Option<&Channel> {
        self.channels()?.ok()?.iter().find(|c| c.slug == slug)
    }

    /// The channel's avatar id, if it has one.
    pub fn avatar(&self, slug: &str) -> Option<&str> {
        self.find(slug).and_then(|c| c.image.as_deref())
    }

    /// Lets App's regression test delay an overview play until a later track selection.
    #[cfg(test)]
    pub(crate) fn insert_pending_tracks(
        &mut self,
        slug: &str,
        receiver: mpsc::Receiver<Result<Vec<Track>, String>>,
    ) {
        self.tracks.insert(
            slug.to_string(),
            Tracks {
                list: None,
                rx: Some(receiver),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(slug: &str) -> Channel {
        Channel {
            slug: slug.into(),
            name: slug.into(),
            description: None,
            image: Some(format!("{slug}-avatar")),
            created_at: None,
            latest_track_at: None,
            track_count: None,
        }
    }

    #[test]
    fn cached_channels_paint_before_a_successful_refresh() {
        let mut library = Library::fixed(Vec::new(), HashMap::new());
        library.channels = None;
        let (tx, rx) = mpsc::channel();
        library.channel_rx = Some(rx);
        assert!(library.channels().is_none());
        tx.send(Ok(vec![channel("cached")])).unwrap();
        tx.send(Ok(vec![channel("fresh")])).unwrap();

        library.poll_updates();
        assert_eq!(library.channels().unwrap().unwrap()[0].slug, "cached");
        library.poll_updates();
        assert_eq!(library.channels().unwrap().unwrap()[0].slug, "fresh");
        assert!(library.find("cached").is_none());
        assert_eq!(library.avatar("fresh"), Some("fresh-avatar"));
    }

    #[test]
    fn failed_refresh_preserves_cached_channels() {
        let mut library = Library::fixed(vec![channel("cached")], HashMap::new());
        let (tx, rx) = mpsc::channel();
        library.channel_rx = Some(rx);
        tx.send(Err("offline".into())).unwrap();

        library.poll_updates();
        assert_eq!(library.channels().unwrap().unwrap()[0].slug, "cached");
    }

    #[test]
    fn failure_without_cached_channels_is_visible() {
        let mut library = Library::fixed(Vec::new(), HashMap::new());
        library.channels = None;
        let (tx, rx) = mpsc::channel();
        library.channel_rx = Some(rx);
        tx.send(Err("offline".into())).unwrap();

        library.poll_updates();
        assert_eq!(library.channels().unwrap().unwrap_err(), "offline");
    }

    #[test]
    fn pending_tracks_keep_their_failure_after_another_request() {
        let mut library = Library::fixed(Vec::new(), HashMap::new());
        let (tx, rx) = mpsc::channel();
        library.insert_pending_tracks("a", rx);
        library.poll_updates();
        assert!(library.tracks("a").is_none());
        tx.send(Err("offline".into())).unwrap();

        library.poll_updates();
        library.request_tracks("a");
        assert_eq!(library.tracks("a").unwrap().unwrap_err(), "offline");
    }
}
