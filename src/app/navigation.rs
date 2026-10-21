//! View navigation and pixel-snapped transitions.
use super::browse::Search;
use egui::Ui;
use std::time::{Duration, Instant};

/// How long a view takes to slide over the one under it, or back off it.
const SLIDE: Duration = Duration::from_millis(280);
/// Every slide eases out on this power, steeper than `ease`, like iOS: nearly there at once, then a long settle.
const STEEP: i32 = 5;

#[derive(Clone)]
pub(super) struct ChannelRef {
    pub(super) slug: String,
    pub(super) name: String,
}

#[derive(Default)]
pub(super) struct Nav {
    pub(super) open_channel: Option<ChannelRef>,
    pub(super) show_channel_info: bool,
    pub(super) info_transition: Option<Slide>,
    pub(super) departing_channel: Option<ChannelRef>,
    pub(super) channel_transition: Option<Slide>,
    pub(super) show_now_playing: bool,
    pub(super) now_playing_transition: Option<Slide>,
    pub(super) search: Option<Search>,
    /// Narrows the open channel, as typed in search: `#dub house`.
    pub(super) filter: Option<String>,
}

impl Nav {
    pub(super) fn open(&mut self, channel: ChannelRef) {
        if self.open_channel.is_none() {
            self.channel_transition = Some(Slide::start(true, self.channel_transition));
        }
        self.open_channel = Some(channel);
        self.show_channel_info = false;
        self.info_transition = None;
        self.set_now_playing(false);
        self.search = None;
        self.filter = None;
    }
    pub(super) fn back(&mut self) {
        if self.show_now_playing {
            self.set_now_playing(false);
        } else if self.search.is_some() {
            self.search = None;
        } else if self.show_channel_info {
            self.set_info(false);
        } else if self.filter.is_some() {
            self.filter = None;
        } else if self.open_channel.is_some() {
            self.info_transition = None;
            self.departing_channel = self.open_channel.take();
            self.channel_transition = Some(Slide::start(false, self.channel_transition));
        }
    }
    pub(super) fn set_now_playing(&mut self, now: bool) {
        if now != self.show_now_playing {
            self.now_playing_transition = Some(Slide::start(now, self.now_playing_transition));
            self.show_now_playing = now;
        }
    }
    pub(super) fn set_info(&mut self, show: bool) {
        let show = self.open_channel.is_some() && show;
        if show != self.show_channel_info {
            self.info_transition = Some(Slide::start(show, self.info_transition));
            self.show_channel_info = show;
        }
    }
    pub(super) fn toggle_info(&mut self) {
        self.set_info(!self.show_channel_info);
    }
    pub(super) fn typing(&self) -> bool {
        self.search.is_some() && !self.show_now_playing
    }
    pub(super) fn channel_list_focused(&self) -> bool {
        self.open_channel.is_none() && self.search.is_none() && !self.show_now_playing
    }
    pub(super) fn finish_transitions(&mut self, ctx: &egui::Context) {
        if self.channel_transition.is_some_and(|s| s.done()) {
            self.channel_transition = None;
            self.departing_channel = None;
        }
        if self.info_transition.is_some_and(|s| s.done()) {
            self.info_transition = None;
        }
        if self.now_playing_transition.is_some_and(|s| s.done()) {
            self.now_playing_transition = None;
        }
        if self.channel_transition.is_some()
            || self.info_transition.is_some()
            || self.now_playing_transition.is_some()
        {
            ctx.request_repaint();
        }
    }
}

/// A view sliding over the one under it (`over`), or back off it.
#[derive(Clone, Copy)]
pub(super) struct Slide {
    at: Instant,
    over: bool,
}

impl Slide {
    /// Turning back halfway picks up from where the view is, rather than jumping to an end.
    fn start(over: bool, running: Option<Slide>) -> Self {
        let from = running.map_or(if over { 0.0 } else { 1.0 }, |s| s.shown());
        let t = 1.0 - (1.0 - if over { from } else { 1.0 - from }).powf(1.0 / STEEP as f32);
        Slide {
            at: Instant::now() - SLIDE.mul_f32(t),
            over,
        }
    }

    /// How much of the view on top is in, from 0 to 1.
    pub(super) fn shown(&self) -> f32 {
        let t = (self.at.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - t).powi(STEEP);
        if self.over { e } else { 1.0 - e }
    }

    fn done(&self) -> bool {
        self.at.elapsed() >= SLIDE
    }
}

/// A sliding view moves in whole pixels, so its text stays crisp.
pub(super) fn snap(ui: &Ui, v: egui::Vec2) -> egui::Vec2 {
    let ppp = ui.ctx().pixels_per_point();
    (v * ppp).round() / ppp
}

/// Quick off the mark, settling at the end.
pub(super) fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::browse::SearchTarget;
    #[test]
    fn turning_back_mid_slide_picks_up_where_it_was() {
        let half = Slide {
            at: Instant::now() - SLIDE / 2,
            over: true,
        };
        let back = Slide::start(false, Some(half));
        assert!((back.shown() - half.shown()).abs() < 0.01);
        assert!(half.shown() > 0.5 && half.shown() < 1.0);
        assert_eq!(Slide::start(true, None).shown().round(), 0.0);
        assert_eq!(Slide::start(false, None).shown().round(), 1.0);
    }

    #[test]
    fn back_peels_now_playing_search_info_tag_then_channel() {
        let mut nav = Nav {
            open_channel: Some(ChannelRef {
                slug: "oskar".into(),
                name: "Radio Oskar".into(),
            }),
            show_channel_info: true,
            search: Some(Search::new(SearchTarget::Channels)),
            show_now_playing: true,
            filter: Some("#dub".into()),
            ..Nav::default()
        };

        nav.back();
        assert!(!nav.show_now_playing);
        assert!(!nav.now_playing_transition.unwrap().over);
        assert!(nav.search.is_some());
        assert!(nav.show_channel_info);
        assert!(nav.open_channel.is_some());
        assert!(nav.channel_transition.is_none());

        nav.back();
        assert!(nav.search.is_none());
        assert!(nav.show_channel_info);
        assert!(nav.open_channel.is_some());
        assert!(nav.channel_transition.is_none());

        nav.back();
        assert!(!nav.show_channel_info);
        assert_eq!(nav.filter.as_deref(), Some("#dub"));

        nav.back();
        assert!(nav.filter.is_none());
        assert!(nav.open_channel.is_some());
        assert!(nav.channel_transition.is_none());

        nav.back();
        assert!(nav.open_channel.is_none());
        assert_eq!(nav.departing_channel.as_ref().unwrap().slug, "oskar");
        assert!(!nav.channel_transition.unwrap().over);
    }

    #[test]
    fn opening_from_search_and_now_playing_preserves_slide_continuity() {
        let channel_slide = Slide {
            at: Instant::now() - SLIDE / 2,
            over: false,
        };
        let now_playing_slide = Slide {
            at: Instant::now() - SLIDE / 2,
            over: true,
        };
        let mut nav = Nav {
            show_channel_info: true,
            search: Some(Search::new(SearchTarget::Channels)),
            show_now_playing: true,
            channel_transition: Some(channel_slide),
            now_playing_transition: Some(now_playing_slide),
            ..Nav::default()
        };

        nav.open(ChannelRef {
            slug: "oskar".into(),
            name: "Radio Oskar".into(),
        });

        assert_eq!(nav.open_channel.as_ref().unwrap().slug, "oskar");
        assert!(!nav.show_channel_info);
        assert!(nav.search.is_none());
        assert!(!nav.show_now_playing);
        let channel = nav.channel_transition.unwrap();
        let now_playing = nav.now_playing_transition.unwrap();
        assert!(channel.over);
        assert!(!now_playing.over);
        assert!((channel.shown() - channel_slide.shown()).abs() < 0.01);
        assert!((now_playing.shown() - now_playing_slide.shown()).abs() < 0.01);
    }

    #[test]
    fn details_requires_a_channel_and_reverses_without_jumping() {
        let mut nav = Nav::default();
        nav.toggle_info();
        assert!(!nav.show_channel_info);
        assert!(nav.info_transition.is_none());
        nav.open(ChannelRef {
            slug: "oskar".into(),
            name: "Radio Oskar".into(),
        });
        nav.toggle_info();
        assert!(nav.show_channel_info);
        assert!(nav.info_transition.unwrap().over);

        let entering = Slide {
            at: Instant::now() - SLIDE / 2,
            over: true,
        };
        nav.info_transition = Some(entering);
        nav.back();
        assert!(!nav.show_channel_info);
        let leaving = nav.info_transition.unwrap();
        assert!(!leaving.over);
        assert!((leaving.shown() - entering.shown()).abs() < 0.01);

        nav.toggle_info();
        assert!(nav.show_channel_info);
        let returning = nav.info_transition.unwrap();
        assert!(returning.over);
        assert!((returning.shown() - leaving.shown()).abs() < 0.01);
    }

    #[test]
    fn leaving_or_reopening_a_channel_discards_its_details_transition() {
        let mut nav = Nav::default();
        nav.open(ChannelRef {
            slug: "oskar".into(),
            name: "Radio Oskar".into(),
        });
        nav.toggle_info();
        nav.back();
        assert!(nav.info_transition.is_some());
        nav.back();
        assert!(nav.open_channel.is_none());
        assert!(!nav.show_channel_info);
        assert!(nav.info_transition.is_none());
        assert!(!nav.channel_transition.unwrap().over);

        nav.open(ChannelRef {
            slug: "oskar".into(),
            name: "Radio Oskar".into(),
        });
        nav.toggle_info();
        nav.open(ChannelRef {
            slug: "other".into(),
            name: "Other channel".into(),
        });
        assert_eq!(nav.open_channel.as_ref().unwrap().slug, "other");
        assert!(!nav.show_channel_info);
        assert!(nav.info_transition.is_none());
    }

    #[test]
    fn finished_details_transition_leaves_the_destination_selected() {
        let mut nav = Nav {
            open_channel: Some(ChannelRef {
                slug: "oskar".into(),
                name: "Radio Oskar".into(),
            }),
            show_channel_info: true,
            info_transition: Some(Slide {
                at: Instant::now() - SLIDE,
                over: true,
            }),
            ..Nav::default()
        };
        nav.finish_transitions(&egui::Context::default());
        assert!(nav.show_channel_info);
        assert!(nav.info_transition.is_none());
        assert_eq!(nav.open_channel.as_ref().unwrap().slug, "oskar");
    }
}
